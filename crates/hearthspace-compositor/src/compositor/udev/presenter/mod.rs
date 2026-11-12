//! Device-local presentation. No caller-side method performs a DRM/GL operation.
mod capture;
mod continuity;
mod cursor;
mod fallback;
mod lifecycle;
mod mailbox;
mod output;
mod render;
mod retirement;
mod scene_element;
mod worker;

#[cfg(test)]
mod buffer_tests;
#[cfg(test)]
mod tests;

use std::{io, sync::Arc, thread::JoinHandle};

use smithay::{
    backend::{
        allocator::{
            Fourcc, Modifier, Slot,
            dmabuf::Dmabuf,
            gbm::{GbmBuffer, GbmDevice},
        },
        drm::{DrmDeviceFd, DrmEventMetadata},
        renderer::{element::Id, sync::SyncPoint, utils::Buffer},
    },
    reexports::{
        calloop::channel,
        drm::control::{Mode, connector, crtc},
    },
    utils::{Physical, Size},
};

use super::{cursor_snapshot::CursorSnapshot, identity::DeviceToken};
use mailbox::Mailbox;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct OutputKey {
    pub device: DeviceToken,
    pub crtc: crtc::Handle,
    pub generation: u64,
}

#[derive(Clone, Debug)]
pub(super) struct OutputConfig {
    pub key: OutputKey,
    pub mode: Mode,
    pub connector: connector::Handle,
    pub name: String,
    pub size: Size<i32, Physical>,
}

/// The client guard is also cloned by Smithay into pending/current plane state,
/// preventing wl_buffer.release until KMS has replaced the scanned-out buffer.
pub(super) enum SceneStorage {
    Composed(Arc<Slot<GbmBuffer>>),
    Client(Buffer),
}

impl SceneStorage {
    pub fn slot(&self) -> Option<&Arc<Slot<GbmBuffer>>> {
        match self {
            Self::Composed(slot) => Some(slot),
            Self::Client(_) => None,
        }
    }
}

pub(super) struct SceneFrame {
    pub output: OutputKey,
    pub sequence: u64,
    pub security_epoch: u64,
    pub lock_generation: Option<u64>,
    // Managed window IDs start at one; keep the optional identity compact.
    pub window_id: Option<std::num::NonZeroU64>,
    pub storage: SceneStorage,
    pub dmabuf: Dmabuf,
    pub ready: SyncPoint,
    pub capture: bool,
}

#[derive(Clone)]
pub(super) struct CursorUpdate {
    pub sequence: u64,
    pub security_epoch: u64,
    pub output: Option<OutputKey>,
    pub snapshot: Option<CursorSnapshot>,
}

pub(super) struct CapturedFrame {
    pub size: Size<i32, Physical>,
    pub pixels: Vec<u8>,
    pub clean_pixels: Vec<u8>,
}

pub(super) enum PresenterEvent {
    Ready,
    Configured {
        key: OutputKey,
        control_generation: u64,
        format: Fourcc,
        modifiers: Vec<Modifier>,
    },
    Presented {
        key: OutputKey,
        submission_sequence: u64,
        scene_sequence: u64,
        cursor_sequence: u64,
        cursor_image: Option<(Id, u64)>,
        security_epoch: u64,
        lock_generation: Option<u64>,
        metadata: DrmEventMetadata,
        scene_changed: bool,
        planes: super::scanout_status::PresentedPlanes,
        capture: Option<Result<CapturedFrame, String>>,
    },
    FrameOpportunity {
        key: OutputKey,
        security_epoch: u64,
        cursor_image: Option<(Id, u64)>,
    },
    SceneDiscarded {
        key: OutputKey,
        scene_sequence: u64,
    },
    SceneCapacityAvailable {
        key: OutputKey,
    },
    Paused {
        control_generation: u64,
    },
    Failed {
        output: Option<OutputKey>,
        error: String,
    },
    Stopped,
}

pub(super) struct Presenter {
    mailbox: Arc<Mailbox>,
    thread: Option<JoinHandle<()>>,
}

impl Presenter {
    // Both threads must share the GBM device's GEM-handle bookkeeping for this
    // DRM file description. A second gbm_create_device over a cloned fd can
    // close handles still owned by the other device when old buffers retire.
    pub fn spawn(
        device: DeviceToken,
        drm_fd: DrmDeviceFd,
        gbm: GbmDevice<DrmDeviceFd>,
    ) -> io::Result<(Self, channel::Channel<PresenterEvent>)> {
        let (sender, receiver) = channel::channel();
        let (ping, wakeup) = smithay::reexports::calloop::ping::make_ping()?;
        let mailbox = Arc::new(Mailbox::new(ping));
        let worker_mailbox = mailbox.clone();
        let thread = std::thread::Builder::new()
            .name(format!("present-{}-{}", device.id, device.incarnation))
            .spawn(move || worker::run(device, drm_fd, gbm, worker_mailbox, wakeup, sender))?;
        Ok((
            Self {
                mailbox,
                thread: Some(thread),
            },
            receiver,
        ))
    }

    pub fn configure(&self, outputs: Vec<OutputConfig>) -> u64 {
        self.mailbox.configure(outputs)
    }

    /// An unaccepted frame remains owned by the caller, including its producer fence.
    pub fn submit_scene(&self, frame: SceneFrame) -> Result<(), SceneFrame> {
        self.mailbox.submit_scene(frame)
    }

    pub fn update_cursor(&self, cursor: CursorUpdate) {
        self.mailbox.update_cursor(cursor);
    }

    pub fn set_security(&self, epoch: u64, lock_generation: Option<u64>) {
        self.mailbox.set_security(epoch, lock_generation);
    }

    pub fn pause(&self) -> u64 {
        self.mailbox.set_active(false)
    }

    pub fn activate(&self) -> u64 {
        self.mailbox.set_active(true)
    }

    pub fn stop(&self) {
        self.mailbox.stop();
    }

    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for Presenter {
    fn drop(&mut self) {
        self.stop();
        // Hotplug callers retain the Presenter until is_finished(). Only final
        // session teardown intentionally waits for an active worker here.
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("native presenter thread panicked");
        }
    }
}
