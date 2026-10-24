//! Keep application scopes owned by this session while using the host manager solely
//! for process tracking. Applications retain the selected session bus and display.
use std::{
    fs, io,
    os::unix::fs::FileTypeExt,
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use crate::{cli::RunMode, session_env::SessionPaths};

mod cleanup;
mod identity;
mod runtime;
pub(crate) use cleanup::shutdown;
use identity::owner_unit;
pub(crate) use runtime::command_path;

pub(crate) const OWNER_ENV: &str = "HEARTHSPACE_APP_SCOPE_UNIT";

pub(crate) struct AppScopes {
    unit: String,
}

impl AppScopes {
    pub(crate) fn start(
        paths: &SessionPaths,
        mode: RunMode,
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
        let guard = match mode {
            RunMode::DevCargo => crate::dev_cargo::prepare_helper(
                "hearthspace-app-scope-guard",
                &crate::session_env::repo_root(),
                &paths.session_dir,
                deadline,
                cancelled,
            )?,
            RunMode::InstalledBinaries => {
                // Never consult the checkout/Cargo in an immutable installed release.
                let current_exe = std::env::current_exe()?;
                binary_dir
                    .unwrap_or_else(|| current_exe.parent().expect("executable directory"))
                    .join("hearthspace-app-scope-guard")
            }
        };
        let owner = Self { unit };
        let pid = std::process::id();
        let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
        let start_time = identity::process_start_time(&stat)
            .ok_or_else(|| io::Error::other("cannot establish supervisor process identity"))?;
        let mut command = cleanup::command(&paths.host_runtime_dir, "systemd-run");
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
        command.arg(format!(
            "--description={}",
            cleanup::description(&paths.session_dir)
        ));
        command
            .arg(&guard)
            .arg(pid.to_string())
            .arg(start_time)
            .arg(&paths.session_dir);
        // Record intent before contacting the manager, including an interrupted start.
        fs::write(paths.session_dir.join(cleanup::OWNER_FILE), &owner.unit)?;
        cleanup::checked_output(&mut command, deadline, cancelled)?;
        while !paths.session_dir.join(identity::READY_FILE).is_file() {
            std::thread::sleep(
                crate::readiness::remaining(deadline, cancelled)?.min(Duration::from_millis(20)),
            );
        }
        let properties =
            cleanup::properties(&paths.host_runtime_dir, &owner.unit, deadline, cancelled)?;
        if properties.get("ActiveState").map(String::as_str) != Some("active") {
            return Err(io::Error::other("app scope owner exited during startup"));
        }
        runtime::prepare(&paths.session_dir, &guard)?;
        Ok(Some(owner))
    }

    pub(crate) fn unit(&self) -> &str {
        &self.unit
    }
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
}
