use std::path::PathBuf;

use smithay::backend::session::Session;
use tracing::{debug, error, info, warn};

use super::{
    ConnectorSync, UdevBackendState,
    connector_policy::refresh_targets,
    device::{UdevDevice, UdevDeviceInfo, create_udev_device, current_device_list},
    identity::DeviceToken,
    insert_libinput_source,
    recovery::insert_drm_source,
};

impl UdevBackendState {
    pub(super) fn device_failed(&mut self, token: DeviceToken) {
        if let Some(device) = self
            .opened_devices
            .get_mut(&token.id)
            .filter(|device| device.token == token)
        {
            device.recovery_needed = true;
            device.mark_all_surfaces_dirty();
            self.refresh_backend_state();
            self.schedule_recovery_timer();
        }
    }

    pub(super) fn pause_session(&mut self) {
        self.gpus.bridges.retire_all();
        self.session_active = false;
        self.drm_commits_paused = true;
        self.kms_devices_active = false;
        self.native_shortcuts.clear();
        if let Some(token) = self.input_source.take() {
            self.loop_handle.remove(token);
        }
        for device in self.opened_devices.values_mut() {
            device.scanout_node.drm_device.pause();
            self.published_outputs.invalidate(
                device
                    .output_surfaces
                    .iter()
                    .map(|output| output.target.connector_name()),
            );
            device.output_surfaces.clear();
            device.active = false;
            device.recovery_needed = true;
        }
        // Dropping each output releases its intermediate and completion together.
        self.schedule_bridge_poll();
        info!("native session paused; DRM commits disabled and Wayland clients remain connected");
    }

    pub(super) fn activate_session(&mut self) -> ConnectorSync {
        if !self.session.is_active() {
            return ConnectorSync::default();
        }
        self.session_active = true;
        if self.input_source.is_none() {
            self.native_shortcuts.clear();
            match insert_libinput_source(&self.loop_handle, self.session.clone(), &self.seat_name) {
                Ok(token) => self.input_source = Some(token),
                Err(error) => error!(%error, "failed to recreate native libinput source"),
            }
        }
        match current_device_list(&self.seat_name) {
            Ok(devices) => {
                let removed: Vec<_> = self
                    .opened_devices
                    .keys()
                    .copied()
                    .filter(|id| !devices.iter().any(|device| device.device_id == *id))
                    .collect();
                for id in removed {
                    self.close_device(id);
                }
                self.devices = devices;
            }
            Err(error) => {
                error!(%error, "failed to re-scan seat DRM devices");
                self.recovery_needed = true;
                return ConnectorSync::default();
            }
        }
        self.open_missing_devices();
        let ids: Vec<_> = self.opened_devices.keys().copied().collect();
        for id in ids {
            // A healthy GPU's pending scanout must survive retries elsewhere.
            let retry = self.opened_devices[&id]
                .output_surfaces
                .iter()
                .any(|output| !output.presentation.presented_once);
            self.refresh_device(id, retry);
        }
        self.refresh_backend_state();
        self.connector_sync()
    }

    pub(super) fn add_or_update_device(&mut self, device_id: u64, path: PathBuf) {
        if self
            .opened_devices
            .get(&device_id)
            .is_some_and(|device| device.path != path)
        {
            self.close_device(device_id);
        }
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
        self.opened_devices
            .values()
            .flat_map(UdevDevice::output_descriptors)
            .collect()
    }

    pub(super) fn connector_sync(&mut self) -> ConnectorSync {
        self.published_outputs.update(self.output_descriptors())
    }

    pub(super) fn handle_device_changed(&mut self, device_id: u64) -> ConnectorSync {
        self.connector_rescan_pending = true;
        if !self.session_active || !self.session.is_active() {
            debug!(device_id, "deferring DRM rescan until session activation");
            return ConnectorSync::default();
        }
        self.open_missing_devices();
        self.refresh_device(device_id, false);
        self.refresh_backend_state();
        self.connector_sync()
    }

    pub(super) fn remove_device(&mut self, device_id: u64) {
        self.devices.retain(|device| device.device_id != device_id);
        self.close_device(device_id);
        self.refresh_backend_state();
    }

    pub(super) fn close_device(&mut self, device_id: u64) {
        let Some(mut device) = self.opened_devices.remove(&device_id) else {
            return;
        };
        self.gpus.bridges.retire_node(device.node);
        self.gpus.bridges.poll();
        self.gpus.incarnations.remove(&device.node);
        if let Some(token) = device.source.take() {
            self.loop_handle.remove(token);
        }
        if device.active && self.session.is_active() {
            device.clear_hardware_cursor();
        }
        self.published_outputs.invalidate(
            device
                .output_surfaces
                .iter()
                .map(|output| output.target.connector_name()),
        );
        device.output_surfaces.clear();
        // Caller-owned intermediates are outside the manager's invalidation.
        // Reset them with their scenes/fences while both contexts still exist.
        for remaining in self.opened_devices.values_mut() {
            for output in &mut remaining.output_surfaces {
                output.reset_scene();
                output.presentation.dirty = true;
            }
        }
        // Invalidate renderer caches while both contexts still exist. Then remove
        // the renderer before the last seat descriptor owner is destroyed.
        if let Err(error) = self.gpus.manager.invalidate_caches() {
            warn!(%error, device_id, "failed to invalidate removed GPU caches");
        }
        self.gpus.manager.as_mut().remove_node(&device.node);
        let _ = self.gpus.manager.devices_mut();
        if self.gpus.scene_node == Some(device.node) {
            self.gpus.select_scene(None);
            self.select_available_scene();
        }
        drop(device);
    }

    pub(super) fn open_missing_devices(&mut self) {
        if !self.session_active || !self.session.is_active() {
            return;
        }
        for info in self.devices.clone() {
            if self.opened_devices.contains_key(&info.device_id) {
                continue;
            }
            self.next_incarnation = self.next_incarnation.wrapping_add(1);
            let token = DeviceToken {
                id: info.device_id,
                incarnation: self.next_incarnation,
            };
            let opened = create_udev_device(
                &mut self.session,
                info.path,
                true,
                token,
                &mut self.gpus,
                &mut self.connector_names,
            );
            match opened {
                Ok((mut device, notifier)) => {
                    match insert_drm_source(&self.loop_handle, token, notifier) {
                        Ok(source) => device.source = Some(source),
                        Err(error) => {
                            error!(%error, device_id = token.id, "failed to register DRM notifier");
                            self.gpus.manager.as_mut().remove_node(&device.node);
                            let _ = self.gpus.manager.devices_mut();
                            continue;
                        }
                    }
                    self.gpus
                        .incarnations
                        .insert(device.node, (token, device.identity.clone()));
                    if self.gpus.scene_node.is_none() {
                        self.gpus.select_scene(Some(device.node));
                    }
                    info!(device_id = token.id, incarnation = token.incarnation,
                        scene_node = ?self.gpus.scene_node, target_node = ?device.node,
                        outputs = device.output_targets.len(), "opened seat DRM device");
                    self.opened_devices.insert(token.id, device);
                }
                Err(error) => {
                    error!(%error, device_id = token.id, "failed to open seat DRM device; retaining healthy GPUs")
                }
            }
        }
    }

    fn refresh_device(&mut self, id: u64, retry_unpresented: bool) -> bool {
        let Some(device) = self.opened_devices.get_mut(&id) else {
            return false;
        };
        let was_active = device.active;
        if !device.active {
            if let Err(error) = device.scanout_node.drm_device.activate(true) {
                error!(%error, device_id = id, "failed to reactivate DRM device");
                device.recovery_needed = true;
                return false;
            }
            device.active = true;
        }
        let targets = match device.connected_output_targets(&mut self.connector_names) {
            Ok((targets, complete)) => {
                device.discovery_complete = complete;
                targets
            }
            Err(error) => {
                error!(%error, device_id = id, "failed to discover native outputs");
                device.discovery_complete = false;
                return false;
            }
        };
        let current = std::mem::replace(&mut device.output_targets, targets.clone());
        let refreshed = refresh_targets(
            &current,
            &targets,
            device.output_surfaces.len() == targets.len(),
            !was_active || retry_unpresented || device.recovery_needed,
            || {
                for output in &device.output_surfaces {
                    self.gpus
                        .bridges
                        .retire_output(output.target.connector_name());
                }
                self.published_outputs.invalidate(
                    device
                        .output_surfaces
                        .iter()
                        .map(|output| output.target.connector_name()),
                );
                match device.rebuild_output_surfaces(targets.clone()) {
                    Ok(()) => true,
                    Err(error) => {
                        error!(%error, device_id = id, "failed to rebuild some KMS output surfaces");
                        false
                    }
                }
            },
        );
        device.recovery_needed = refreshed.recovery_needed;
        refreshed.rebuilt
    }

    pub(super) fn refresh_backend_state(&mut self) {
        self.select_available_scene();
        self.kms_devices_active = self.session_active
            && self
                .opened_devices
                .values()
                .any(|device| device.active && device.has_output_surfaces());
        self.drm_commits_paused =
            !self.session_active || !self.kms_devices_active || self.gpus.scene_node.is_none();
        self.recovery_needed = self.input_source.is_none()
            || self.devices.iter().any(|info| {
                self.opened_devices
                    .get(&info.device_id)
                    .is_none_or(|device| {
                        !device.active || !device.discovery_complete || device.recovery_needed
                    })
            });
        self.connector_rescan_pending = self.recovery_needed;
        self.schedule_bridge_poll();
    }

    fn select_available_scene(&mut self) {
        let selected = self
            .gpus
            .scene_node
            .filter(|node| {
                self.opened_devices
                    .values()
                    .any(|device| device.node == *node && device.active)
            })
            .or_else(|| {
                self.opened_devices
                    .values()
                    .find(|device| device.active)
                    .map(|device| device.node)
            });
        if selected == self.gpus.scene_node {
            return;
        }
        self.gpus.select_scene(selected);
        for device in self.opened_devices.values_mut() {
            for output in &mut device.output_surfaces {
                output.reset_scene();
                output.damage_tracker =
                    smithay::backend::renderer::damage::OutputDamageTracker::new(
                        output.target.output_size(),
                        1.0,
                        smithay::utils::Transform::Normal,
                    );
                output.presentation.dirty = true;
            }
        }
    }
}
