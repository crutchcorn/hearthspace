//! Publish only the environment needed by applications activated on our bus.
use std::{io, path::Path, sync::atomic::AtomicBool, time::Instant};

use crate::session_env::{SessionEnv, SessionPaths};

mod cleanup;
mod environment;
mod publish;

/// Retire shared activation values before removing the session directory.
/// The scope guard can replay the same record after supervisor failure.
pub(crate) struct ActivationEnvironment {
    record: Option<cleanup::Record>,
}

impl ActivationEnvironment {
    pub(crate) fn retire(&mut self) -> io::Result<()> {
        if let Some(record) = &self.record {
            cleanup::retire_from_session_dir(&record.session_dir, Some(record))?;
            self.record = None;
        }
        Ok(())
    }
}

impl Drop for ActivationEnvironment {
    fn drop(&mut self) {
        if self.retire().is_err() {
            tracing::warn!("activation retirement failed; retaining cleanup record");
        }
    }
}

/// Retry rollback when publication failed before returning its owned guard.
pub(crate) fn retire_persisted(directory: &Path) -> io::Result<()> {
    cleanup::retire_from_session_dir(directory, None)
}

pub(crate) fn update_activation_environment(
    paths: &SessionPaths,
    session: &SessionEnv,
    private_session_bus: bool,
    startup_deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<Option<ActivationEnvironment>> {
    let deadline = startup_deadline.min(Instant::now() + cleanup::operations::TIMEOUT);
    cleanup::operations::remaining(deadline, cancelled)?;
    let profile = if private_session_bus {
        crate::profile_env::for_session(session)?
    } else {
        Vec::new()
    };
    let values = environment::values(paths, session, private_session_bus, &profile)?;
    let bus = values["DBUS_SESSION_BUS_ADDRESS"].clone();
    if private_session_bus {
        cleanup::operations::update(&bus, &values, false, deadline, cancelled)?;
        return Ok(None);
    }
    publish::shared(paths, &bus, values, deadline, cancelled).map(Some)
}

#[cfg(test)]
mod tests;
