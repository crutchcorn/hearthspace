use smithay::{
    backend::session::{Session, libseat::LibSeatSession},
    reexports::rustix::fs::OFlags,
    utils::DeviceFd,
};
use std::{os::fd::OwnedFd, path::Path};
use tracing::warn;

/// Keeps the original descriptor alive until session.close can retire libseat's
/// device record. All DRM, GBM and notifier clones must be dropped first.
pub(super) struct SessionFd {
    fd: Option<DeviceFd>,
    session: LibSeatSession,
}

impl SessionFd {
    pub(super) fn open(
        session: &mut LibSeatSession,
        path: &Path,
        flags: OFlags,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            fd: Some(DeviceFd::from(session.open(path, flags)?)),
            session: session.clone(),
        })
    }

    pub(super) fn fd(&self) -> DeviceFd {
        self.fd.as_ref().expect("live session descriptor").clone()
    }
}

impl Drop for SessionFd {
    fn drop(&mut self) {
        let Some(fd) = self.fd.take() else {
            return;
        };
        let owned: Result<OwnedFd, _> = fd.try_into();
        match owned {
            Ok(fd) => {
                if let Err(error) = self.session.close(fd) {
                    warn!(%error, "failed to release native session device");
                }
            }
            Err(_) => warn!("native device retained a descriptor beyond session ownership"),
        }
    }
}
