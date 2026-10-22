//! Keep application scopes owned by this session while using the host manager solely
//! for process tracking. Application D-Bus and display access stay private.
use std::{
    collections::{BTreeSet, HashMap},
    fs, io,
    os::unix::fs::FileTypeExt,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use crate::{bounded_command, session_env::SessionPaths};

mod identity;
mod runtime;
use identity::owner_unit;
pub(crate) use runtime::command_path;

pub(crate) const OWNER_ENV: &str = "HEARTHSPACE_APP_SCOPE_UNIT";

pub(crate) struct AppScopes {
    unit: String,
    runtime: PathBuf,
    host_runtime: PathBuf,
}

impl AppScopes {
    pub(crate) fn start(
        paths: &SessionPaths,
        binary_dir: Option<&Path>,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> io::Result<Option<Self>> {
        if !fs::metadata(paths.host_runtime_dir.join("systemd/private"))
            .is_ok_and(|meta| meta.file_type().is_socket())
        {
            tracing::warn!("host user manager unavailable; Snap scope launch disabled");
            return Ok(None);
        }
        let unit = owner_unit(&paths.session_dir)?;
        let current_exe = std::env::current_exe()?;
        let guard = binary_dir
            .unwrap_or_else(|| current_exe.parent().expect("executable directory"))
            .join("hearthspace-app-scope-guard");
        let owner = Self {
            unit,
            runtime: paths.session_dir.clone(),
            host_runtime: paths.host_runtime_dir.clone(),
        };
        let pid = std::process::id();
        let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
        let start_time = identity::process_start_time(&stat)
            .ok_or_else(|| io::Error::other("cannot establish supervisor process identity"))?;
        let mut command = owner.command("systemd-run");
        command.args([
            "--user",
            "--collect",
            "--quiet",
            "--no-ask-password",
            "--service-type=exec",
            "--property=Restart=no",
            "--property=TimeoutStopSec=1",
        ]);
        command.arg(format!("--unit={}", owner.unit));
        command.arg(format!("--description={}", owner.description()));
        command
            .arg(&guard)
            .arg(pid.to_string())
            .arg(start_time)
            .arg(&owner.runtime);
        // Record intent before contacting the manager, including an interrupted start.
        fs::write(owner.runtime.join("app-scope-owner"), &owner.unit)?;
        checked_output(&mut command, deadline, cancelled)?;
        while !owner.runtime.join(identity::READY_FILE).is_file() {
            std::thread::sleep(
                crate::readiness::remaining(deadline, cancelled)?.min(Duration::from_millis(20)),
            );
        }
        let properties = owner.properties(&owner.unit, deadline, cancelled)?;
        if properties.get("ActiveState").map(String::as_str) != Some("active") {
            return Err(io::Error::other("app scope owner exited during startup"));
        }
        runtime::prepare(&owner.runtime, &guard)?;
        Ok(Some(owner))
    }

    pub(crate) fn unit(&self) -> &str {
        &self.unit
    }

    pub(crate) fn shutdown(&self) -> io::Result<()> {
        fs::write(self.runtime.join(identity::CLOSING_FILE), "")?;
        let deadline = Instant::now() + Duration::from_secs(4);
        let cancelled = AtomicBool::new(false);
        let mut dependents = scope_records(&self.runtime)?;
        let properties = self.properties(&self.unit, deadline, &cancelled)?;
        if properties.get("LoadState").map(String::as_str) != Some("not-found") {
            if properties.get("Description") != Some(&self.description()) {
                return Err(io::Error::other(
                    "app scope owner identity changed; refusing stop",
                ));
            }
            dependents.extend(
                properties
                    .get("BoundBy")
                    .into_iter()
                    .flat_map(|value| value.split_whitespace().map(str::to_owned)),
            );
            let mut stop = self.command("systemctl");
            stop.args(["--user", "--no-ask-password", "stop", &self.unit]);
            checked_output(&mut stop, deadline, &cancelled)?;
        }
        dependents.insert(self.unit.clone());
        loop {
            let mut all_stopped = true;
            for unit in &dependents {
                all_stopped &= stopped(&self.properties(unit, deadline, &cancelled)?);
            }
            if all_stopped {
                return Ok(());
            }
            std::thread::sleep(
                crate::readiness::remaining(deadline, &cancelled)?.min(Duration::from_millis(50)),
            );
        }
    }

    fn description(&self) -> String {
        format!("Hearthspace app scopes for {}", self.runtime.display())
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        command.env("XDG_RUNTIME_DIR", &self.host_runtime).env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/bus", self.host_runtime.display()),
        );
        command
    }

    fn properties(
        &self,
        unit: &str,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> io::Result<HashMap<String, String>> {
        let mut command = self.command("systemctl");
        command.args([
            "--user",
            "--no-ask-password",
            "show",
            unit,
            "--property=LoadState,ActiveState,Description,BoundBy",
        ]);
        // systemctl show may report a not-found unit with a nonzero status.
        let output = bounded_command::output(&mut command, deadline, cancelled)?;
        let properties = parse_properties(&String::from_utf8_lossy(&output.stdout));
        if !output.status.success()
            && properties.get("LoadState").map(String::as_str) != Some("not-found")
        {
            return Err(io::Error::other(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ));
        }
        Ok(properties)
    }
}

fn checked_output(
    command: &mut Command,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    let result = bounded_command::output(command, deadline, cancelled)?;
    if !result.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&result.stderr).into_owned(),
        ));
    }
    Ok(())
}

fn parse_properties(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.into(), value.into()))
        .collect()
}

fn stopped(properties: &HashMap<String, String>) -> bool {
    properties.get("LoadState").map(String::as_str) == Some("not-found")
        || matches!(
            properties.get("ActiveState").map(String::as_str),
            Some("inactive" | "failed")
        )
}

fn scope_records(runtime: &Path) -> io::Result<BTreeSet<String>> {
    let directory = runtime.join("app-scopes");
    match fs::symlink_metadata(&directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Ok(meta) if meta.file_type().is_dir() => {}
        _ => return Err(io::Error::other("invalid app scope record directory")),
    }
    let mut units = BTreeSet::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if units.len() >= 512 || !entry.file_type()?.is_file() || !identity::valid_scope_name(&name)
        {
            return Err(io::Error::other("invalid or excessive app scope records"));
        }
        units.insert(name);
    }
    Ok(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_name_is_derived_without_trusting_user_records() {
        assert_eq!(
            owner_unit(Path::new("/run/user/1000/hearthspace-native-abc123")).unwrap(),
            "hearthspace-apps-hearthspace-native-abc123.service"
        );
        assert!(owner_unit(Path::new("/tmp/name.service;other")).is_err());
        assert!(owner_unit(Path::new("/")).is_err());
    }

    #[test]
    fn empty_or_transitional_properties_never_prove_cleanup() {
        assert!(!stopped(&HashMap::new()));
        assert!(!stopped(&parse_properties("ActiveState=deactivating\n")));
        assert!(stopped(&parse_properties("LoadState=not-found\n")));
        assert!(stopped(&parse_properties("ActiveState=inactive\n")));
    }
}
