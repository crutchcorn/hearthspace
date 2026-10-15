use nix::sys::signal::Signal;
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};
use tracing::{info, warn};

use crate::{
    bounded_command,
    cli::{Args, Backend},
    compositor::{CompositorBackend, select_compositor_backend},
    process::{
        ManagedProcess, ProcessSpec, poll_process, terminate_process,
        wait_then_kill_process_with_timeout,
    },
    readiness::remaining,
    session_env::{SessionEnv, SessionPaths},
};

mod owned_pid;
use owned_pid::OwnedPid;

const DEFINITIONS: &[(&str, &str, Option<&str>)] = &[
    (
        "permission-store",
        "/usr/libexec/xdg-permission-store",
        Some("org.freedesktop.impl.portal.PermissionStore"),
    ),
    (
        "document-portal",
        "/usr/libexec/xdg-document-portal",
        Some("org.freedesktop.portal.Documents"),
    ),
    (
        "portal-gtk",
        "/usr/libexec/xdg-desktop-portal-gtk",
        Some("org.freedesktop.impl.portal.desktop.gtk"),
    ),
    (
        "desktop-portal",
        "/usr/libexec/xdg-desktop-portal",
        Some("org.freedesktop.portal.Desktop"),
    ),
    (
        "polkit-agent",
        "/usr/libexec/polkit-mate-authentication-agent-1",
        None,
    ),
];

pub(crate) fn enabled(args: &Args) -> bool {
    !args.no_desktop_services
        && args.private_session_bus
        && match args.backend {
            Some(Backend::Native) => true,
            Some(Backend::Nested) => false,
            None => select_compositor_backend() == CompositorBackend::NativeTty,
        }
}

pub(crate) fn prepare(paths: &SessionPaths) -> io::Result<()> {
    let config = paths.session_dir.join("share/xdg-desktop-portal");
    fs::create_dir_all(&config)?;
    fs::write(
        config.join("hearthspace-portals.conf"),
        include_str!("../../../data/xdg-desktop-portal/hearthspace-portals.conf"),
    )?;
    // Prevent a crashed or not-yet-started portal from being auto-spawned outside
    // supervision. These overrides belong only to this private session bus.
    let services = paths.session_dir.join("dbus-1/services");
    fs::create_dir_all(&services)?;
    for bus in DEFINITIONS
        .iter()
        .filter_map(|(_, _, bus)| *bus)
        .chain(std::iter::once("org.freedesktop.Notifications"))
    {
        fs::write(
            services.join(format!("{bus}.service")),
            format!("[D-BUS Service]\nName={bus}\nExec=/usr/bin/false\n"),
        )?;
    }
    Ok(())
}

#[derive(Default)]
pub(crate) struct DesktopServices {
    entries: Vec<Entry>,
    runtime: PathBuf,
    bus: String,
}

struct Entry {
    spec: ProcessSpec,
    bus_name: Option<&'static str>,
    child: Option<ManagedProcess>,
    adopted: Option<OwnedPid>,
    restarts_left: usize,
}

impl DesktopServices {
    pub(crate) fn start(
        &mut self,
        paths: &SessionPaths,
        session: &SessionEnv,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime = paths.session_dir.clone();
        self.bus = session
            .vars
            .iter()
            .find(|(key, _)| key == "HEARTHSPACE_SESSION_BUS_ADDRESS")
            .map(|(_, value)| value.clone())
            .ok_or("desktop services require a private bus")?;
        let profile = crate::profile_env::for_session(session)?;
        for &(name, program, bus_name) in DEFINITIONS {
            let mut envs = session.vars.clone();
            envs.extend(profile.iter().cloned());
            envs.push(("DBUS_SESSION_BUS_ADDRESS".into(), self.bus.clone()));
            envs.push(("GDK_BACKEND".into(), "wayland".into()));
            envs.push(("GIO_USE_VFS".into(), "local".into()));
            if matches!(name, "permission-store" | "document-portal") {
                let data = portal_data_dir(&paths.session_dir)?;
                fs::create_dir_all(&data)?;
                envs.push(("XDG_DATA_HOME".into(), data.to_string_lossy().into_owned()));
            }
            let spec = ProcessSpec {
                name,
                program: program.into(),
                args: Vec::new(),
                envs,
                env_removals: vec![
                    "DISPLAY".into(),
                    "WAYLAND_SOCKET".into(),
                    "AT_SPI_BUS_ADDRESS".into(),
                ],
                current_dir: None,
            };
            let adopted = if let Some(bus_name) = bus_name {
                owner_pid(&self.bus, bus_name, deadline, cancelled)?
                    .map(|pid| OwnedPid::adopt(pid, Path::new(program), &self.runtime, &self.bus))
                    .transpose()?
            } else {
                None
            };
            let child = if adopted.is_none() {
                Some(ManagedProcess::spawn(spec.clone(), 0)?)
            } else {
                None
            };
            self.entries.push(Entry {
                spec,
                bus_name,
                child,
                adopted,
                restarts_left: 2,
            });
            self.entries
                .last_mut()
                .unwrap()
                .wait_ready(&self.bus, deadline, cancelled)?;
            info!(name, "private desktop service ready");
        }
        Ok(())
    }

    pub(crate) fn poll(&mut self, cancelled: &AtomicBool) -> bool {
        let mut changed = false;
        for entry in &mut self.entries {
            if entry.child.is_none() && entry.adopted.is_none() && entry.restarts_left == 0 {
                continue;
            }
            match entry.running() {
                Ok(true) => continue,
                Ok(false) => warn!(name = entry.spec.name, "desktop service exited"),
                Err(error) => {
                    warn!(name = entry.spec.name, %error, "could not poll desktop service")
                }
            }
            entry.stop();
            changed = true;
            if entry.restarts_left == 0 {
                continue;
            }
            entry.restarts_left -= 1;
            let result = (|| {
                entry.child = Some(ManagedProcess::spawn(entry.spec.clone(), 0)?);
                entry.wait_ready(
                    &self.bus,
                    Instant::now() + Duration::from_secs(3),
                    cancelled,
                )
            })();
            if let Err(error) = result {
                warn!(name = entry.spec.name, %error, "desktop service restart failed");
                entry.stop();
            }
        }
        changed
    }

    pub(crate) fn shutdown(&mut self) {
        for entry in self.entries.iter_mut().rev() {
            entry.terminate();
        }
        for entry in self.entries.iter_mut().rev() {
            entry.reap();
        }
        self.entries.clear();
    }

    pub(crate) fn process_json_fields(&self) -> String {
        self.entries
            .iter()
            .map(|entry| format!(",\"{}\":{}", entry.spec.name, entry.pid().unwrap_or(0)))
            .collect()
    }
}

impl Entry {
    fn pid(&self) -> Option<u32> {
        self.child
            .as_ref()
            .map(ManagedProcess::pid)
            .or_else(|| self.adopted.as_ref().map(OwnedPid::pid))
    }
    fn running(&mut self) -> io::Result<bool> {
        if let Some(adopted) = &self.adopted {
            return Ok(adopted.running());
        }
        if self.child.is_none() {
            return Ok(false);
        }
        poll_process(&mut self.child)
            .map(|status| status.is_none())
            .map_err(|error| io::Error::other(error.to_string()))
    }
    fn wait_ready(
        &mut self,
        bus: &str,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let deadline = deadline.min(Instant::now() + Duration::from_secs(5));
        if self.bus_name.is_none() {
            thread::sleep(remaining(deadline, cancelled)?.min(Duration::from_millis(100)));
            return if self.running()? {
                Ok(())
            } else {
                Err("authentication agent exited during startup".into())
            };
        }
        loop {
            remaining(deadline, cancelled)?;
            if !self.running()? {
                return Err(format!("{} exited during startup", self.spec.name).into());
            }
            if owner_pid(bus, self.bus_name.unwrap(), deadline, cancelled)? == self.pid() {
                return Ok(());
            }
            thread::sleep(remaining(deadline, cancelled)?.min(Duration::from_millis(10)));
        }
    }
    fn terminate(&mut self) {
        terminate_process(&mut self.child);
        if let Some(adopted) = &self.adopted {
            adopted.signal(Signal::SIGTERM);
        }
    }
    fn reap(&mut self) {
        wait_then_kill_process_with_timeout(&mut self.child, Duration::from_millis(500));
        if let Some(adopted) = &self.adopted {
            adopted.wait_then_kill();
        }
        self.child = None;
        self.adopted = None;
    }
    fn stop(&mut self) {
        self.terminate();
        self.reap();
    }
}

fn owner_pid(
    bus: &str,
    name: &str,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<Option<u32>> {
    let result = bounded_command::output(
        Command::new("busctl").args([
            "--address",
            bus,
            "--timeout=0.25",
            "call",
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetConnectionUnixProcessID",
            "s",
            name,
        ]),
        deadline.min(Instant::now() + Duration::from_millis(300)),
        cancelled,
    )?;
    if !result.status.success() {
        return Ok(None);
    }
    let output = String::from_utf8_lossy(&result.stdout);
    let fields = output.split_whitespace().collect::<Vec<_>>();
    match fields.as_slice() {
        ["u", pid] => pid.parse().map(Some).map_err(io::Error::other),
        _ => Err(io::Error::other("invalid D-Bus service owner reply")),
    }
}

fn portal_data_dir(runtime: &Path) -> io::Result<PathBuf> {
    if env::var_os("HEARTHSPACE_ISOLATE_APP_PROFILES").is_some_and(|value| value == "1") {
        return Ok(runtime.join("portal-data"));
    }
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|home| home.join("hearthspace/portals"))
        .ok_or_else(|| io::Error::other("HOME or XDG_DATA_HOME is required for portal data"))
}

#[cfg(test)]
mod tests;
