//! Main-thread scene storage. KMS buffers and their presentation live in the worker.
use super::super::{
    presentation::OutputPresentation,
    presenter::{OutputKey, SceneFrame},
    transfer::{SceneBuffer, cache::OutputTransfer},
};
use super::{KmsOutputTarget, UdevDevice};
use crate::compositor::capture::OutputMetadata;
use smithay::{
    backend::{
        allocator::{
            Fourcc, Modifier, Slot, Swapchain,
            gbm::{GbmAllocator, GbmBuffer, GbmBufferFlags},
        },
        drm::DrmDeviceFd,
        renderer::damage::OutputDamageTracker,
    },
    utils::Transform,
};

pub(in crate::compositor::udev) struct KmsOutputSurface {
    pub(in crate::compositor::udev) target: KmsOutputTarget,
    pub(in crate::compositor::udev) key: OutputKey,
    pub(in crate::compositor::udev) pool: Swapchain<GbmAllocator<DrmDeviceFd>>,
    pub(in crate::compositor::udev) format: Fourcc,
    pub(in crate::compositor::udev) damage_tracker: OutputDamageTracker,
    pub(in crate::compositor::udev) presentation: OutputPresentation,
    pub(in crate::compositor::udev) scene: Option<SceneBuffer>,
    pub(in crate::compositor::udev) transfer: OutputTransfer,
    pub(in crate::compositor::udev) failed_slot: Option<std::sync::Arc<Slot<GbmBuffer>>>,
    pub(in crate::compositor::udev) rejected_scene: Option<SceneFrame>,
    pub(in crate::compositor::udev) capture: Option<(u64, OutputMetadata, std::time::Instant)>,
}

impl KmsOutputSurface {
    pub(in crate::compositor::udev) fn reset_scene(&mut self) {
        self.scene = None;
        self.transfer.reset();
    }
}

impl UdevDevice {
    pub(in crate::compositor::udev) fn configured(
        &mut self,
        key: OutputKey,
        control_generation: u64,
        format: Fourcc,
        modifiers: Vec<Modifier>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if !self.active
            || key.device != self.token
            || key.generation != self.generation
            || control_generation != self.control_generation
        {
            return Ok(false);
        }
        if self.output_surfaces.iter().any(|o| o.key == key) {
            return Ok(false);
        }
        let Some(target) = self
            .output_targets
            .iter()
            .find(|t| t.crtc == key.crtc)
            .cloned()
        else {
            return Ok(false);
        };
        let modifiers: Vec<_> = modifiers
            .into_iter()
            .filter(|modifier| {
                self.renderer_formats
                    .iter()
                    .any(|f| f.code == format && f.modifier == *modifier)
            })
            .collect();
        if modifiers.is_empty() {
            return Err("presenter scene format is not renderable on its producer".into());
        }
        let size = target.output_size();
        self.output_surfaces.push(KmsOutputSurface {
            target,
            key,
            format,
            pool: Swapchain::new(
                GbmAllocator::new(self.gbm_device.clone(), GbmBufferFlags::RENDERING),
                size.w as u32,
                size.h as u32,
                format,
                modifiers,
            ),
            damage_tracker: OutputDamageTracker::new(size, 1.0, Transform::Normal),
            presentation: OutputPresentation::new(key),
            scene: None,
            transfer: OutputTransfer::default(),
            capture: None,
            failed_slot: None,
            rejected_scene: None,
        });
        self.configuration_pending = self.output_surfaces.len() != self.output_targets.len();
        self.recovery_needed = self.configuration_pending;
        self.recovery_not_before =
            std::time::Instant::now() + super::super::recovery::CONFIRMATION_GRACE;
        Ok(true)
    }
}
