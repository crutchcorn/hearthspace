use super::*;
use crate::cli::RunMode;

pub(crate) struct Configuration<'a> {
    pub(crate) mode: RunMode,
    pub(crate) binary_dir: Option<&'a Path>,
    pub(crate) private_session_bus: bool,
}

// Only external process boundaries are replaceable in the isolated broker tests.
// Production always resolves the backend from screencast's fixed install paths.
struct LaunchPlan {
    bus: String,
    private_session_bus: bool,
    capture: ProcessSpec,
    services: Vec<(ProcessSpec, Option<&'static str>)>,
    systemctl: PathBuf,
}

fn selected_bus(session: &SessionEnv) -> io::Result<&str> {
    session
        .vars
        .iter()
        .find(|(key, _)| key == "HEARTHSPACE_SESSION_BUS_ADDRESS")
        .map(|(_, value)| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| io::Error::other("desktop services require an explicit session bus"))
}

pub(super) fn service_spec(
    paths: &SessionPaths,
    session: &SessionEnv,
    name: &'static str,
    program: &str,
) -> io::Result<ProcessSpec> {
    let mut envs = session.vars.clone();
    envs.extend(crate::profile_env::for_session(session)?);
    envs.push((
        "DBUS_SESSION_BUS_ADDRESS".into(),
        selected_bus(session)?.into(),
    ));
    envs.push(("GDK_BACKEND".into(), "wayland".into()));
    envs.push(("GIO_USE_VFS".into(), "local".into()));
    if name == "portal-gtk" {
        envs.push(("GTK_USE_PORTAL".into(), "0".into()));
    }
    if matches!(name, "permission-store" | "document-portal") {
        let data = portal_data_dir(&paths.session_dir)?;
        fs::create_dir_all(&data)?;
        envs.push(("XDG_DATA_HOME".into(), data.to_string_lossy().into_owned()));
    }
    Ok(ProcessSpec {
        name,
        program: program.into(),
        args: Vec::new(),
        envs,
        env_removals: ["DISPLAY", "WAYLAND_SOCKET", "AT_SPI_BUS_ADDRESS"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        current_dir: None,
    })
}

impl DesktopServices {
    pub(crate) fn start(
        &mut self,
        paths: &SessionPaths,
        session: &SessionEnv,
        config: Configuration<'_>,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let bus = selected_bus(session)?.to_owned();
        screencast::require_unowned(&bus, deadline, cancelled)?;
        let capture = screencast::spec(
            paths,
            session,
            &screencast::backend()?,
            &screencast::picker(config.mode, config.binary_dir)?,
        )?;
        let services = DEFINITIONS
            .iter()
            .filter(|(_, _, bus)| config.private_session_bus || bus.is_none())
            .map(|&(name, program, bus)| Ok((service_spec(paths, session, name, program)?, bus)))
            .collect::<io::Result<_>>()?;
        self.start_plan(
            paths,
            LaunchPlan {
                bus,
                private_session_bus: config.private_session_bus,
                capture,
                services,
                systemctl: "systemctl".into(),
            },
            deadline,
            cancelled,
        )
    }

    fn start_plan(
        &mut self,
        paths: &SessionPaths,
        plan: LaunchPlan,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime = paths.session_dir.clone();
        self.bus = plan.bus;
        let result = (|| {
            // Ready before either private frontend startup or shared refresh.
            self.start_entry(
                plan.capture,
                Some(screencast::BUS_NAME),
                deadline,
                cancelled,
            )?;
            if !plan.private_session_bus {
                self.shared.start_with_systemctl(
                    &plan.systemctl,
                    &paths.host_runtime_dir,
                    &self.bus,
                    deadline,
                    cancelled,
                )?;
            }
            for (spec, bus) in plan.services {
                self.start_entry(spec, bus, deadline, cancelled)?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.shutdown();
        }
        result
    }

    fn start_entry(
        &mut self,
        spec: ProcessSpec,
        bus_name: Option<&'static str>,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let adopted = if let Some(name) = bus_name.filter(|name| *name != screencast::BUS_NAME) {
            owner_pid(&self.bus, name, deadline, cancelled)?
                .map(|pid| OwnedPid::adopt(pid, Path::new(&spec.program), &self.runtime, &self.bus))
                .transpose()?
        } else {
            None
        };
        let mut entry = Entry {
            spec,
            bus_name,
            child: None,
            adopted,
            restarts_left: 2,
        };
        if entry.adopted.is_none() {
            entry.spawn(&self.bus, deadline, cancelled)?;
        }
        self.entries.push(entry);
        let entry = self.entries.last_mut().unwrap();
        entry.wait_ready(&self.bus, deadline, cancelled)?;
        tracing::info!(name = entry.spec.name, "supervised desktop service ready");
        Ok(())
    }
}

#[cfg(test)]
mod tests;
