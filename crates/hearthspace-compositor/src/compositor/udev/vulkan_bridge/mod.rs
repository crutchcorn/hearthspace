//! DMA-BUF image-to-Linear-buffer copy on the canonical scene GPU.
//!
//! Independently implemented against Vulkan external-memory and synchronization
//! contracts. No renderer, KMS, scene, allocation policy, or private protocol
//! belongs here. Callers provide compositor-owned, accurately described DMA-BUFs
//! and serialize every external access with the acquire/release fences.
//!
//! DMA_BUF's External Memory Handle Types Compatibility table places no UUID
//! restrictions on cross-device/driver sharing and requires compatibility
//! queries. VkImportMemoryFdInfoKHR-fd-00668 nevertheless retains contradictory
//! same-device wording. This adapter exercises the common driver-supported
//! interpretation; it does not claim that the contradictory VUID is resolved.
//! https://docs.vulkan.org/spec/latest/chapters/capabilities.html#external-memory-handle-types-compatibility

mod admission;
mod buffer;
mod device;
mod error;
mod image;
mod layout;
mod memory;
mod submit;
mod sync;

pub(crate) use buffer::LinearTarget;
pub(crate) use device::DeviceIdentity;
pub(crate) use error::{BridgeError, SubmitFailure};
pub(crate) use image::SourceImage;
pub(crate) use layout::{BufferLayout, FORMATS};
pub(crate) use memory::AllocationInfo;
pub(crate) use sync::{CopyAcquire, CopyCompletion, NativeFence};

use device::Device;
use smithay::backend::{
    allocator::{Format, dmabuf::Dmabuf},
    drm::DrmNode,
};
use std::rc::Rc;
use submit::Pending;

const MAX_PENDING: usize = 8;

pub(crate) struct VulkanBridge {
    device: Rc<Device>,
    formats: Vec<Format>,
    pending: Vec<Pending>,
    enabled: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Progress {
    pub pending: usize,
    /// Completion permits destruction, but shared pixels/sync state are invalid.
    pub device_lost: bool,
}

/// A retired bridge accepts no new work and polls without blocking. Keep it in
/// the backend's event-loop retirement list until pending==0. Do not recreate
/// the same failed device incarnation while unresolved work remains.
pub(crate) struct RetiredBridge(VulkanBridge);

impl VulkanBridge {
    pub fn new(source: DrmNode) -> Result<Self, BridgeError> {
        if !Self::admission_open() {
            return Err(BridgeError::Retired);
        }
        let device = Device::new(source)?;
        let formats = device.supports.iter().map(|s| s.format).collect();
        Ok(Self {
            device,
            formats,
            pending: Vec::with_capacity(MAX_PENDING),
            enabled: true,
        })
    }

    pub fn identity(&self) -> &DeviceIdentity {
        &self.device.identity
    }
    pub fn source_formats(&self) -> &[Format] {
        &self.formats
    }

    pub fn admission_open() -> bool {
        admission::IMPORTS.is_open()
    }

    pub fn ready(&mut self) -> Result<bool, BridgeError> {
        self.reap_completed()?;
        self.check_enabled()?;
        Ok(self.pending.len() < MAX_PENDING)
    }

    pub fn stop(&mut self) {
        self.enabled = false;
    }

    /// Import the actual, unmodified export of a source-local GBM allocation.
    /// The caller owns its GLES producer and must wait for source_released before
    /// its next write. Never pass a foreign GL texture handle through this API.
    pub fn import_source(&mut self, dma: &Dmabuf) -> Result<SourceImage, BridgeError> {
        self.check_enabled()?;
        SourceImage::import(self.device.clone(), dma)
    }

    /// Import a target-owned Linear allocation, separate from the KMS swapchain.
    /// The caller must supply the last target reader's fence before each write.
    pub fn import_linear_target(&mut self, dma: &Dmabuf) -> Result<LinearTarget, BridgeError> {
        self.check_enabled()?;
        LinearTarget::import(self.device.clone(), dma)
    }

    /// Source and target storage must be disjoint. Full-image copies preserve
    /// storage orientation and byte format; presentation transforms stay in GLES.
    pub fn submit(
        &mut self,
        source: &SourceImage,
        target: &LinearTarget,
        acquire: CopyAcquire,
    ) -> Result<CopyCompletion, SubmitFailure> {
        let reject = |error| SubmitFailure {
            error,
            submitted: false,
        };
        self.reap_completed().map_err(reject)?;
        self.check_enabled().map_err(reject)?;
        if self.pending.len() >= MAX_PENDING {
            return Err(reject(BridgeError::Busy));
        }
        if !Rc::ptr_eq(&source.0.device, &self.device)
            || !Rc::ptr_eq(&target.0.device, &self.device)
        {
            return Err(reject(BridgeError::Invalid(
                "resource belongs to another bridge incarnation",
            )));
        }
        let a = source.layout();
        let b = target.layout();
        if a.format.code != b.format.code || a.width != b.width || a.height != b.height {
            return Err(reject(BridgeError::Unsupported(
                "copy requires identical dimensions and Fourcc",
            )));
        }
        if source.0._dma == target.0._dma {
            return Err(reject(BridgeError::Invalid("source and destination alias")));
        }
        let mut pending =
            Pending::prepare(self.device.clone(), source, target, acquire).map_err(reject)?;
        let result = pending.submit();
        // Keep the host fence and resources even when sync-file export fails.
        self.pending.push(pending);
        result.map_err(|error| {
            self.enabled = false;
            SubmitFailure {
                error,
                submitted: true,
            }
        })
    }

    pub fn reap_completed(&mut self) -> Result<Progress, BridgeError> {
        let mut error = None;
        self.pending.retain_mut(|job| match job.poll() {
            Ok(done) => !done,
            Err(e) => {
                error.get_or_insert(e);
                true
            }
        });
        if self.device.lost.get() || error.is_some() {
            self.enabled = false;
        }
        if let Some(error) = error {
            return Err(error);
        }
        Ok(Progress {
            pending: self.pending.len(),
            device_lost: self.device.lost.get(),
        })
    }

    pub fn retire(mut self) -> RetiredBridge {
        self.stop();
        RetiredBridge(self)
    }

    fn check_enabled(&self) -> Result<(), BridgeError> {
        if !self.enabled || self.device.lost.get() || !Self::admission_open() {
            Err(BridgeError::Retired)
        } else {
            Ok(())
        }
    }
}

impl RetiredBridge {
    pub fn poll(&mut self) -> Result<Progress, BridgeError> {
        self.0.reap_completed()
    }
}
