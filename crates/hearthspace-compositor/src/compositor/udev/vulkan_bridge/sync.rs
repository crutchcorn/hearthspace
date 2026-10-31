use super::{BridgeError, device::Device};
use ash::vk;
use smithay::{
    backend::renderer::sync::SyncPoint,
    reexports::rustix::event::{PollFd, PollFlags, Timespec, poll},
};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};

/// A completed dependency or an owned Linux sync_file, never an opaque fence fd.
#[derive(Debug)]
pub(crate) enum NativeFence {
    Signaled,
    SyncFile(OwnedFd),
}

impl NativeFence {
    pub fn from_sync_point(sync: &SyncPoint) -> Result<Self, BridgeError> {
        if !sync.contains_fence() {
            return Ok(Self::Signaled);
        }
        // Preserve exported dependencies even when already reached. This keeps
        // the steady-state path explicitly synchronized in the artifact probe.
        if let Some(fd) = sync.export() {
            return Ok(Self::SyncFile(fd));
        }
        if sync.is_reached() {
            return Ok(Self::Signaled);
        }
        Err(BridgeError::Unsupported(
            "incomplete GLES fence cannot export a sync_file",
        ))
    }

    pub fn try_clone(&self) -> Result<Self, BridgeError> {
        match self {
            Self::Signaled => Ok(Self::Signaled),
            Self::SyncFile(fd) => fd
                .try_clone()
                .map(Self::SyncFile)
                .map_err(|e| BridgeError::io("duplicate completion sync_file", e)),
        }
    }

    /// Completion alone does not establish successful rendering. In particular,
    /// the caller must separately invalidate pixels after device loss.
    pub fn is_signaled(&self) -> Result<bool, BridgeError> {
        let Self::SyncFile(fd) = self else {
            return Ok(true);
        };
        let mut descriptors = [PollFd::new(fd, PollFlags::IN)];
        match poll(
            &mut descriptors,
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            }),
        ) {
            Err(smithay::reexports::rustix::io::Errno::INTR) => return Ok(false),
            Err(e) => return Err(BridgeError::io("poll sync_file", e)),
            Ok(_) => {}
        }
        let flags = descriptors[0].revents();
        if flags.intersects(PollFlags::ERR | PollFlags::NVAL | PollFlags::HUP) {
            return Err(BridgeError::Invalid("sync_file poll reported an error"));
        }
        Ok(flags.contains(PollFlags::IN))
    }
}

pub(crate) struct CopyAcquire {
    pub source_ready: NativeFence,
    pub target_released: NativeFence,
}

pub(crate) struct CopyCompletion {
    pub source_released: NativeFence,
    pub target_ready: NativeFence,
}

pub(super) fn import(
    device: &Device,
    semaphore: vk::Semaphore,
    fence: NativeFence,
) -> Result<(), BridgeError> {
    let fd = match &fence {
        NativeFence::Signaled => -1,
        NativeFence::SyncFile(fd) => fd.as_raw_fd(),
    };
    let info = vk::ImportSemaphoreFdInfoKHR::default()
        .semaphore(semaphore)
        .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
        .flags(vk::SemaphoreImportFlags::TEMPORARY)
        .fd(fd);
    unsafe { device.semaphore_fd.import_semaphore_fd(&info) }
        .map_err(|e| BridgeError::vk("import acquisition sync_file", e))?;
    if let NativeFence::SyncFile(fd) = fence {
        let _consumed_fd = fd.into_raw_fd();
    }
    Ok(())
}

pub(super) fn export(
    device: &Device,
    semaphore: vk::Semaphore,
) -> Result<CopyCompletion, BridgeError> {
    let info = vk::SemaphoreGetFdInfoKHR::default()
        .semaphore(semaphore)
        .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
    let fd = unsafe { device.semaphore_fd.get_semaphore_fd(&info) }
        .map_err(|e| BridgeError::vk("export copy sync_file", e))?;
    let source_released = if fd == -1 {
        NativeFence::Signaled
    } else if fd >= 0 {
        NativeFence::SyncFile(unsafe { OwnedFd::from_raw_fd(fd) })
    } else {
        return Err(BridgeError::Invalid("invalid exported sync_file fd"));
    };
    // SYNC_FD export transfers the semaphore payload. Export exactly once and
    // duplicate the resulting fd for the two independent consumers.
    let target_ready = source_released.try_clone()?;
    Ok(CopyCompletion {
        source_released,
        target_ready,
    })
}
