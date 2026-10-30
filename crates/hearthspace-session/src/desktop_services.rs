use nix::sys::signal::Signal;
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};
use tracing::warn;

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
mod screencast;
mod shared;
mod startup;
use owned_pid::OwnedPid;
pub(crate) use startup::Configuration;

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
        && match args.backend {
            Some(Backend::Native) => true,
            Some(Backend::Nested) => false,
            None => select_compositor_backend() == CompositorBackend::NativeTty,
        }
}

pub(crate) fn prepare(paths: &SessionPaths, private_session_bus: bool) -> io::Result<()> {
    let config = paths.session_dir.join("share/xdg-desktop-portal");
    fs::create_dir_all(&config)?;
    fs::write(
        config.join("hearthspace-portals.conf"),
        include_str!("../../../data/xdg-desktop-portal/hearthspace-portals.conf"),
    )?;
    fs::create_dir_all(config.join("portals"))?;
    fs::write(
        config.join("portals/hearthspace.portal"),
        include_str!("../../../data/xdg-desktop-portal/hearthspace.portal"),
    )?;
    fs::write(config.join("portals/wlr.portal"), screencast::DESCRIPTOR)?;
    if !private_session_bus {
        return Ok(());
    }
    // Prevent a crashed or not-yet-started portal from being auto-spawned outside
    // supervision. These overrides belong only to this private session bus.
    let services = paths.session_dir.join("dbus-1/services");
    fs::create_dir_all(&services)?;
    for bus in DEFINITIONS
        .iter()
        .filter_map(|(_, _, bus)| *bus)
        .chain(std::iter::once("org.freedesktop.Notifications"))
        .chain(std::iter::once(crate::appearance_portal::NAME))
        .chain(std::iter::once(screencast::BUS_NAME))
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
    shared: shared::SharedServices,
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
                let deadline = Instant::now() + Duration::from_secs(3);
                entry.spawn(&self.bus, deadline, cancelled)?;
                entry.wait_ready(&self.bus, deadline, cancelled)
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
        // User services are borrowed; systemd alone controls their lifetime.
        self.shared.clear();
    }

    pub(crate) fn process_json_fields(&self) -> String {
        let owned: String = self
            .entries
            .iter()
            .map(|entry| format!(",\"{}\":{}", entry.spec.name, entry.pid().unwrap_or(0)))
            .collect();
        owned + &self.shared.process_json_fields()
    }
}

impl Entry {
    fn spawn(
        &mut self,
        bus: &str,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.bus_name == Some(screencast::BUS_NAME) {
            screencast::require_unowned(bus, deadline, cancelled)?;
        }
        self.child = Some(ManagedProcess::spawn(self.spec.clone(), 0)?);
        Ok(())
    }

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
            let owner = owner_pid(bus, self.bus_name.unwrap(), deadline, cancelled)?;
            if owner == self.pid() {
                return Ok(());
            }
            if self.bus_name == Some(screencast::BUS_NAME)
                && let Some(owner) = owner
            {
                return Err(screencast::foreign_owner(owner).into());
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
