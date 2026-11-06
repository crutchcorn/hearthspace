use std::{io, path::PathBuf};

use smithay::{
    backend::{
        allocator::{Format, gbm::GbmDevice},
        drm::{DrmDeviceFd, DrmNode},
        udev::{UdevBackend, all_gpus},
    },
    reexports::{calloop::RegistrationToken, rustix::fs::stat},
};
use tracing::{debug, warn};

mod open;
mod pools;
mod session_fd;
mod targets;

use super::{
    identity::{ConnectorNames, DeviceToken},
    presenter::{OutputConfig, OutputKey, Presenter},
};
pub(super) use open::create_udev_device;
pub(super) use pools::KmsOutputSurface;
pub(super) use targets::KmsOutputTarget;

pub(super) struct UdevDevice {
    pub(super) path: PathBuf,
    pub(super) identity: String,
    pub(super) token: DeviceToken,
    pub(super) node: DrmNode,
    pub(super) source: Option<RegistrationToken>,
    pub(super) active: bool,
    pub(super) recovery_needed: bool,
    pub(super) discovery_complete: bool,
    pub(super) configuration_pending: bool,
    pub(super) recovery_not_before: std::time::Instant,
    pub(super) generation: u64,
    pub(super) control_generation: u64,
    pub(super) output_targets: Vec<KmsOutputTarget>,
    pub(super) output_surfaces: Vec<KmsOutputSurface>,
    // Drop/join the worker before releasing any seat ownership. Ordinary removal
    // keeps this entire record retired until the worker has finished.
    pub(super) presenter: Presenter,
    pub(super) gbm_device: GbmDevice<DrmDeviceFd>,
    pub(super) renderer_formats: Vec<Format>,
    pub(super) drm_fd: DrmDeviceFd,
    _session_fd: session_fd::SessionFd,
}

#[derive(Clone)]
pub(super) struct UdevDeviceInfo {
    pub(super) device_id: u64,
    pub(super) path: PathBuf,
}

impl UdevDevice {
    pub(super) fn output_descriptors(&self) -> Vec<super::super::OutputDescriptor> {
        self.output_surfaces
            .iter()
            .map(|s| s.target.output_descriptor())
            .collect()
    }

    pub(super) fn connected_output_targets(
        &self,
        names: &mut ConnectorNames,
    ) -> Result<(Vec<KmsOutputTarget>, bool), Box<dyn std::error::Error>> {
        targets::discover_outputs(&self.drm_fd, &self.path, &self.identity, names)
    }

    pub(super) fn rebuild_output_surfaces(&mut self, targets: Vec<KmsOutputTarget>) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("output generation exhausted");
        self.output_surfaces.clear();
        self.configuration_pending = !targets.is_empty();
        self.recovery_needed = self.configuration_pending;
        self.recovery_not_before = std::time::Instant::now() + super::recovery::CONFIRMATION_GRACE;
        self.control_generation = self.presenter.configure(
            targets
                .iter()
                .map(|target| OutputConfig {
                    key: OutputKey {
                        device: self.token,
                        crtc: target.crtc,
                        generation: self.generation,
                    },
                    mode: target.mode,
                    connector: target.connector,
                    name: target.connector_name().to_owned(),
                    size: target.output_size(),
                })
                .collect(),
        );
    }

    pub(super) fn has_output_surfaces(&self) -> bool {
        !self.output_surfaces.is_empty()
    }
    pub(super) fn mark_all_surfaces_dirty(&mut self) {
        for output in &mut self.output_surfaces {
            output.presentation.dirty = true;
        }
    }
    pub(super) fn any_frame_pending(&self) -> bool {
        self.output_surfaces
            .iter()
            .any(|o| o.presentation.pending())
    }
    pub(super) fn any_frame_dirty(&self) -> bool {
        self.output_surfaces.iter().any(|o| o.presentation.dirty)
    }
}

pub(super) fn initial_device_list(udev_backend: &UdevBackend) -> Vec<UdevDeviceInfo> {
    udev_backend
        .device_list()
        .map(|(device_id, path)| UdevDeviceInfo {
            device_id,
            path: path.to_path_buf(),
        })
        .collect()
}

pub(super) fn current_device_list(seat_name: &str) -> io::Result<Vec<UdevDeviceInfo>> {
    all_gpus(seat_name)?
        .into_iter()
        .map(|path| {
            let device_id = stat(&path)?.st_rdev as u64;
            Ok(UdevDeviceInfo { device_id, path })
        })
        .collect()
}

pub(super) fn log_device_list(seat_name: &str, devices: &[UdevDeviceInfo]) {
    if devices.is_empty() {
        warn!(seat = seat_name, "no DRM devices found for seat");
        return;
    }

    for device in devices {
        debug!(seat = seat_name, device_id = device.device_id, path = %device.path.display(), "found DRM device");
    }
}
