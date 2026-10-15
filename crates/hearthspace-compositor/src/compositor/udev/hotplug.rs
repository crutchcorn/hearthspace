use std::{io, path::PathBuf};

use smithay::backend::udev::primary_gpu;
use tracing::{debug, error, info, warn};

use super::{
    ConnectorSync, UdevBackendState,
    connector_policy::targets_need_rebuild,
    device::{
        UdevDevice, UdevDeviceInfo, create_udev_device, current_device_list, log_device_list,
    },
    insert_libinput_source,
};

impl UdevBackendState {
    pub(super) fn pause_session(&mut self) {
        self.session_active = false;
        self.drm_commits_paused = true;
        self.kms_devices_active = false;
        self.repaint_pending = false;
        self.frame_pending = false;
        self.frame_dirty = true;
        if let Some(token) = self.input_source.take() {
            self.loop_handle.remove(token);
            info!("native libinput source removed for session pause");
        }
        if let Some(device) = &mut self.primary_device {
            device.scanout_node.drm_device.pause();
            device.output_surfaces.clear();
            device.active = false;
        }
        info!("native session paused; DRM commits disabled and Wayland clients remain connected");
    }

    pub(super) fn activate_session(&mut self) -> ConnectorSync {
        self.session_active = true;
        self.connector_rescan_pending = true;

        if self.primary_device.is_none()
            && let Err(error) = self.open_primary_device()
        {
            error!(%error, "failed to open primary DRM device after session activation");
        }

        if self.input_source.is_none() {
            match insert_libinput_source(&self.loop_handle, self.session.clone(), &self.seat_name) {
                Ok(token) => {
                    self.input_source = Some(token);
                    info!("native libinput source recreated after session activation");
                }
                Err(error) => {
                    error!(%error, "failed to recreate native libinput source after session activation");
                }
            }
        }

        if let Err(error) = self.rescan_devices() {
            error!(%error, "failed to re-scan DRM devices after session activation");
        }

        let Some(device) = self.primary_device.as_mut() else {
            self.kms_devices_active = false;
            self.drm_commits_paused = true;
            self.repaint_pending = false;
            self.frame_pending = false;
            self.frame_dirty = true;
            warn!("native session activated but no primary DRM device is available");
            return ConnectorSync::default();
        };

        device.active = true;
        if let Err(error) = device.scanout_node.drm_device.activate(true) {
            error!(%error, "failed to reactivate primary DRM device after session activation");
        }

        device.output_targets = device.connected_output_targets();
        if let Err(error) = device.rebuild_output_surfaces(device.output_targets.clone()) {
            error!(%error, "failed to rebuild KMS output surfaces after session activation");
        }

        self.kms_devices_active = device.has_output_surfaces();
        self.drm_commits_paused = !self.kms_devices_active;
        self.connector_rescan_pending = false;
        self.repaint_pending = self.kms_devices_active;
        self.frame_pending = false;
        self.frame_dirty = true;

        info!(
            repaint_pending = self.repaint_pending,
            "native session activated; DRM devices reactivated"
        );

        ConnectorSync {
            descriptors: Some(device.output_descriptors()),
        }
    }

    pub(super) fn add_or_update_device(&mut self, device_id: u64, path: PathBuf) {
        match self
            .devices
            .iter_mut()
            .find(|device| device.device_id == device_id)
        {
            Some(device) => device.path = path,
            None => self.devices.push(UdevDeviceInfo { device_id, path }),
        }
        self.connector_rescan_pending = true;
    }

    pub(super) fn output_descriptors(&self) -> Vec<super::super::OutputDescriptor> {
        self.primary_device
            .as_ref()
            .map(UdevDevice::output_descriptors)
            .unwrap_or_default()
    }

    pub(super) fn handle_device_changed(&mut self, device_id: u64) -> ConnectorSync {
        self.connector_rescan_pending = true;
        // A disconnected last monitor pauses commits, but the active seat
        // must still rescan to discover its replacement. Only VT pause defers.
        if !self.session_active {
            debug!(
                device_id,
                "deferring DRM device change while native session is paused"
            );
            return ConnectorSync::default();
        }

        if let Err(error) = self.rescan_devices() {
            error!(%error, "failed to re-scan DRM devices after device change");
        }

        let Some(device) = self.primary_device.as_mut() else {
            return ConnectorSync::default();
        };
        if device.scanout_node.drm_fd.dev_id().ok() != Some(device_id) {
            debug!(
                device_id,
                "changed DRM device is not the selected primary device"
            );
            return ConnectorSync {
                descriptors: Some(device.output_descriptors()),
            };
        }

        let previous_target = device.output_target.clone();
        let next_targets = device.connected_output_targets();
        let next_target = next_targets.first().cloned();
        if !targets_need_rebuild(
            &device.output_targets,
            &next_targets,
            device.output_surfaces.len() == next_targets.len(),
        ) {
            debug!("primary DRM connector state re-scanned; selected output is unchanged");
            self.connector_rescan_pending = false;
            device.output_targets = next_targets;
            return ConnectorSync {
                descriptors: Some(device.output_descriptors()),
            };
        }

        info!(previous_target = ?previous_target, next_target = ?next_target, "DRM connector selection changed; rebuilding KMS output surfaces");
        device.output_targets = next_targets;
        if let Err(error) = device.rebuild_output_surfaces(device.output_targets.clone()) {
            error!(%error, "failed to rebuild KMS output surfaces");
        }

        self.kms_devices_active = self.session_active && device.has_output_surfaces();
        self.drm_commits_paused = !self.session_active || !device.has_output_surfaces();
        self.connector_rescan_pending = false;
        self.frame_pending = false;
        self.frame_dirty = self.kms_devices_active;
        self.repaint_pending = self.kms_devices_active;

        ConnectorSync {
            descriptors: Some(device.output_descriptors()),
        }
    }

    pub(super) fn remove_device(&mut self, device_id: u64) {
        let Some(index) = self
            .devices
            .iter()
            .position(|device| device.device_id == device_id)
        else {
            return;
        };
        let removed = self.devices.remove(index);
        if self
            .primary_device
            .as_ref()
            .is_some_and(|device| device.path == removed.path)
        {
            self.primary_device = None;
            self.kms_devices_active = false;
            self.repaint_pending = false;
            warn!("primary DRM device removed; KMS state marked inactive");
        }
        self.connector_rescan_pending = true;
    }

    fn rescan_devices(&mut self) -> io::Result<()> {
        self.devices = current_device_list(&self.seat_name)?;
        log_device_list(&self.seat_name, &self.devices);
        Ok(())
    }

    fn open_primary_device(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(path) = primary_gpu(&self.seat_name)? else {
            return Ok(());
        };
        let (device, _notifier) = create_udev_device(&mut self.session, path, self.session_active)?;
        self.primary_device = Some(device);
        Ok(())
    }
}
