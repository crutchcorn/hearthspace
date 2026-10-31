use super::super::super::vulkan_bridge::{
    BridgeError, LinearTarget, NativeFence, RetiredBridge, SourceImage, VulkanBridge,
};
use smithay::{
    backend::{allocator::dmabuf::Dmabuf, renderer::gles::GlesTexture},
    utils::{Physical, Size},
};

pub(super) const EXTERNAL_LIMIT: usize = 8;

pub(in crate::compositor::udev) struct Resources {
    pub bridge: Option<VulkanBridge>,
    pub retired: Option<RetiredBridge>,
    pub source: SourceImage,
    pub target: LinearTarget,
    pub source_dma: Dmabuf,
    pub _target_dma: Dmabuf,
    pub target_texture: GlesTexture,
    pub size: Size<i32, Physical>,
    pub source_released: NativeFence,
    pub target_released: NativeFence,
    /// A GLES operation started but no trustworthy completion could be exported.
    pub unknown_external: bool,
    pub holds: Vec<(GlesTexture, Dmabuf)>,
}

pub(in crate::compositor::udev) struct Slot {
    pub resources: Option<Resources>,
    pub complete: bool,
    pending: usize,
    reported_loss: bool,
}

impl Slot {
    pub fn new(resources: Resources) -> Self {
        Self {
            resources: Some(resources),
            complete: true,
            pending: 0,
            reported_loss: false,
        }
    }

    pub fn retire(&mut self) {
        if let Some(resources) = &mut self.resources
            && let Some(bridge) = resources.bridge.take()
        {
            resources.retired = Some(bridge.retire());
        }
    }

    pub fn poll(&mut self) -> Result<(), BridgeError> {
        let Some(resources) = &mut self.resources else {
            return Ok(());
        };
        self.complete = false;
        self.pending = 1; // retain polling if the driver cannot report a count
        let progress = match &mut resources.bridge {
            Some(bridge) => bridge.reap_completed()?,
            None => resources.retired.as_mut().expect("retired bridge").poll()?,
        };
        self.pending = progress.pending;
        if progress.device_lost && !self.reported_loss {
            self.reported_loss = true;
            return Err(BridgeError::Retired);
        }
        if resources.unknown_external {
            return Ok(());
        }
        let reached = (|| {
            Ok::<_, BridgeError>(
                resources.source_released.is_signaled()?
                    && resources.target_released.is_signaled()?,
            )
        })();
        match reached {
            Ok(reached) => {
                self.complete = progress.pending == 0 && reached;
                if self.complete {
                    resources.holds.clear();
                }
            }
            Err(error) => {
                resources.unknown_external = true;
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn needs_poll(&self) -> bool {
        !self.complete
            && self
                .resources
                .as_ref()
                .is_some_and(|r| !r.unknown_external || self.pending != 0)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        if !self.complete {
            let _ = self.poll();
        }
        if !self.complete {
            // Slots remain counted while retired. Only final backend teardown
            // reaches this path; neither unknown GLES work nor pending Vulkan
            // commands can lose their backing allocations on runtime exit.
            std::mem::forget(self.resources.take());
            tracing::error!("quarantined unresolved Vulkan transfer slot until process exit");
        }
    }
}
