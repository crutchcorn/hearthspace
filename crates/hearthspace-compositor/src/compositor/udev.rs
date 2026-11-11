use std::collections::BTreeMap;

mod connector_policy;
mod cursor_publication;
mod cursor_snapshot;
mod device;
mod event_cutoff;
mod gpu;
mod hotplug;
mod identity;
mod input;
mod native_shortcuts;
mod output_sync;
mod presentation;
mod presenter;
mod presenter_events;
mod recovery;
mod renderer_preference;
mod rendering;
mod retirement;
mod scanout_status;
mod startup;
mod transfer;

use device::{UdevDevice, UdevDeviceInfo};
use input::insert_libinput_source;
use native_shortcuts::NativeShortcuts;
use output_sync::{ConnectorSync, PublishedOutputs};
pub use startup::run_udev;

use smithay::{
    backend::{
        renderer::{damage::OutputDamageTracker, gles::GlesRenderer},
        session::{Session, libseat::LibSeatSession},
    },
    reexports::calloop::{LoopHandle, RegistrationToken},
    utils::Transform,
};
use tracing::{info, warn};

pub(super) struct UdevBackendState {
    session: LibSeatSession,
    seat_name: String,
    session_active: bool,
    loop_handle: LoopHandle<'static, super::CalloopData>,
    input_source: Option<RegistrationToken>,
    recovery_source: Option<RegistrationToken>,
    recovery_needed: bool,
    recovery_exhausted: bool,
    opened_devices: BTreeMap<u64, UdevDevice>,
    retired_devices: Vec<UdevDevice>,
    retirement_source: Option<RegistrationToken>,
    cursor_publication: cursor_publication::CursorPublication,
    security_epoch: u64,
    lock_generation: Option<u64>,
    next_scene_sequence: u64,
    devices: Vec<UdevDeviceInfo>,
    renderer_preference: renderer_preference::StartupRendererPreference,
    gpus: gpu::GpuState,
    connector_names: identity::ConnectorNames,
    published_outputs: PublishedOutputs,
    next_incarnation: u64,
    kms_devices_active: bool,
    drm_commits_paused: bool,
    connector_rescan_pending: bool,
    input_event_count: u64,
    native_shortcuts: NativeShortcuts,
}

fn udev_backend_mut(data: &mut super::CalloopData) -> Option<&mut UdevBackendState> {
    match &mut data.backend {
        super::Backend::Udev(backend) => Some(backend),
        #[cfg(feature = "winit")]
        super::Backend::Winit(_) | super::Backend::Headless(_) => None,
        #[cfg(not(feature = "winit"))]
        super::Backend::Headless(_) => None,
    }
}

fn apply_connector_sync(data: &mut super::CalloopData, connector_sync: ConnectorSync) {
    if let super::Backend::Udev(backend) = &data.backend {
        data.state
            .set_native_lock_outputs_available(backend.native_outputs_ready());
    }
    if let super::Backend::Udev(backend) = &mut data.backend {
        backend.gpus.refresh_app(&mut data.state);
    }
    let dh = data.display.handle();
    if !connector_sync.apply(&mut data.state, &mut data.captures, &dh) {
        if let super::Backend::Udev(backend) = &data.backend
            && backend.native_outputs_ready()
        {
            // Recovery can restore availability with an unchanged (even empty)
            // advertised set. Reconcile it before considering lock confirmation.
            data.state.reconfigure_lock_outputs();
        }
        return;
    }
    data.damage_tracker =
        OutputDamageTracker::new(data.state.output_size(), 1.0, Transform::Normal);
    data.full_redraw = data.full_redraw.max(1);
}

impl UdevBackendState {
    /// Discovery completeness is distinct from successful presentation. A failed
    /// secondary device must not make the lock or recovery barrier vacuously ready.
    pub(in crate::compositor) fn native_outputs_ready(&self) -> bool {
        self.session_active
            && self.session.is_active()
            && !self.recovery_needed
            && self.devices.iter().all(|info| {
                self.opened_devices
                    .get(&info.device_id)
                    .is_some_and(|device| {
                        device.active
                            && device.discovery_complete
                            && !device.recovery_needed
                            && device.output_surfaces.len() == device.output_targets.len()
                    })
            })
    }

    /// Acquire once before consuming any client import requests. A recovered
    /// seat may still have no usable canonical renderer; secondary readiness
    /// does not prevent imports once this renderer is available.
    pub(in crate::compositor) fn acquire_import_renderer(
        &mut self,
    ) -> Result<Option<impl AsMut<GlesRenderer> + '_>, Box<dyn std::error::Error>> {
        if !self.session_active || !self.session.is_active() {
            return Ok(None);
        }
        let Some(node) = self.gpus.scene_node else {
            return Ok(None);
        };
        Ok(Some(self.gpus.manager.single_renderer(&node)?))
    }

    fn log_summary(&self) {
        info!(session_active = self.session_active, kms_devices_active = self.kms_devices_active,
            drm_commits_paused = self.drm_commits_paused, devices = self.devices.len(),
            opened_devices = self.opened_devices.len(), scene_node = ?self.gpus.scene_node,
            connector_rescan_pending = self.connector_rescan_pending,
            frame_pending = self.opened_devices.values().any(UdevDevice::any_frame_pending),
            frame_dirty = self.opened_devices.values().any(UdevDevice::any_frame_dirty),
            input_event_count = self.input_event_count,
            "native backend state");
    }
}

impl Drop for UdevBackendState {
    fn drop(&mut self) {
        if let Some(token) = self.input_source.take() {
            self.loop_handle.remove(token);
        }
        if let Some(token) = self.recovery_source.take() {
            self.loop_handle.remove(token);
        }
        let ids: Vec<_> = self.opened_devices.keys().copied().collect();
        for id in ids {
            self.close_device(id);
        }
        if let Some(token) = self.retirement_source.take() {
            self.loop_handle.remove(token);
        }
        // Only final session teardown joins; event-loop removal is asynchronous.
        self.retired_devices.clear();
        if let Err(error) = self.gpus.manager.invalidate_caches() {
            warn!(%error, "native renderer cleanup failed");
        }
    }
}
