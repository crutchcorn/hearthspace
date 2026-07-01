use std::{io, path::PathBuf, time::Duration};

mod device;
mod input;

use device::{
    KmsOutputTarget, UdevDevice, UdevDeviceInfo, create_udev_device, current_device_list,
    initial_device_list, log_device_list,
};
use input::log_input_event;

use smithay::{
    backend::{
        allocator::Buffer,
        allocator::dmabuf::Dmabuf,
        drm::DrmEvent,
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{Bind, ImportDma, damage::OutputDamageTracker},
        session::{Event as SessionEvent, Session, libseat::LibSeatSession},
        udev::{UdevBackend, UdevEvent, primary_gpu},
    },
    reexports::drm::control::crtc,
    reexports::{
        calloop::LoopHandle, calloop::RegistrationToken, input::Libinput, wayland_server::Display,
    },
    utils::{Physical, Size, Transform},
};
use tracing::{debug, error, info, trace, warn};

use crate::{
    RunOptions,
    config::{HEADLESS_OUTPUT_HEIGHT, HEADLESS_OUTPUT_WIDTH},
};

pub(super) struct UdevBackendState {
    session: LibSeatSession,
    seat_name: String,
    session_active: bool,
    // The first native milestone opens and renders through exactly one DRM
    // device. `devices` below is discovery state for hotplug logging only.
    loop_handle: LoopHandle<'static, super::CalloopData>,
    input_source: Option<RegistrationToken>,
    primary_device: Option<UdevDevice>,
    devices: Vec<UdevDeviceInfo>,
    kms_devices_active: bool,
    drm_commits_paused: bool,
    connector_rescan_pending: bool,
    repaint_pending: bool,
    frame_pending: bool,
    frame_dirty: bool,
    input_event_count: u64,
    emergency_exit_ctrl_pressed: bool,
    emergency_exit_alt_pressed: bool,
}

#[derive(Default)]
struct ConnectorSync {
    descriptors: Vec<super::OutputDescriptor>,
    primary: Option<super::OutputDescriptor>,
}

pub fn run_udev(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    info!(?options, "initializing native udev compositor backend");
    let termination_signals = super::create_termination_signals()?;
    let (mut session, session_notifier) = LibSeatSession::new()?;
    let seat_name = session.seat();
    info!(seat = seat_name, "native backend acquired seat");

    let udev_backend = UdevBackend::new(&seat_name)?;
    let devices = initial_device_list(&udev_backend);
    log_device_list(&seat_name, &devices);

    let primary_device = match primary_gpu(&seat_name)? {
        Some(path) => {
            info!(path = %path.display(), "selected primary DRM device");
            for device in devices.iter().filter(|device| device.path != path) {
                debug!(device_id = device.device_id, path = %device.path.display(), "ignoring secondary DRM device for now");
            }
            let active = session.is_active();
            let (device, notifier) = create_udev_device(&mut session, path, active)?;
            Some((device, notifier))
        }
        None => {
            warn!(seat = seat_name, "no primary DRM device available for seat");
            None
        }
    };

    let (primary_device, drm_notifier) = match primary_device {
        Some((device, notifier)) => (Some(device), Some(notifier)),
        None => (None, None),
    };

    let session_active = session.is_active();
    let display: Display<super::App> = Display::new()?;
    let dh = display.handle();
    let output_target = primary_device
        .as_ref()
        .and_then(|device| device.output_target.as_ref());
    let output_size = output_target
        .map(KmsOutputTarget::output_size)
        .unwrap_or_else(|| {
            Size::<i32, Physical>::from((HEADLESS_OUTPUT_WIDTH, HEADLESS_OUTPUT_HEIGHT))
        });
    let primary_output = if let Some(target) = output_target {
        let descriptor = target.output_descriptor();
        super::create_output_with_properties(
            &dh,
            descriptor.name,
            descriptor.properties,
            descriptor.size,
            descriptor.scale,
            descriptor.refresh,
        )
    } else {
        super::create_output(&dh, output_size, 1)
    };
    let (dmabuf_formats, main_device) = primary_device
        .as_ref()
        .map(|device| {
            let formats = device
                .render_node
                .renderer
                .dmabuf_formats()
                .into_iter()
                .collect::<Vec<_>>();
            let main_device = device.render_node.drm_fd.dev_id().ok();
            if let Some(main_device) = main_device {
                debug!(main_device, path = %device.path.display(), "native dmabuf feedback main device selected");
            }
            (formats, main_device)
        })
        .unwrap_or_default();
    let dmabuf = super::create_dmabuf_global(&dh, dmabuf_formats, main_device)?;
    let super::AppInit {
        display,
        mut event_loop,
        handle,
        mut app,
    } = super::initialize_app(
        display,
        options,
        primary_output,
        dmabuf,
        termination_signals,
    )?;
    let output_descriptors = primary_device
        .as_ref()
        .map(UdevDevice::output_descriptors)
        .unwrap_or_default();
    app.sync_connector_outputs(&dh, output_descriptors);
    app.enable_software_cursor();

    handle.insert_source(session_notifier, |event, _, data| match event {
        SessionEvent::PauseSession => {
            if let Some(backend) = udev_backend_mut(data) {
                backend.pause_session();
            }
        }
        SessionEvent::ActivateSession => {
            let connector_sync = udev_backend_mut(data)
                .map(|backend| backend.activate_session())
                .unwrap_or_default();
            apply_connector_sync(data, connector_sync);
            data.state.request_redraw();
        }
    })?;

    handle.insert_source(udev_backend, |event, _, data| match event {
        UdevEvent::Added { device_id, path } => {
            info!(device_id, path = %path.display(), "DRM device added");
            let output_descriptors = if let Some(backend) = udev_backend_mut(data) {
                backend.add_or_update_device(device_id, path);
                backend.output_descriptors()
            } else {
                Vec::new()
            };
            let dh = data.display.handle();
            data.state.sync_connector_outputs(&dh, output_descriptors);
            data.state.reconcile_pointer_after_output_geometry_change();
            data.full_redraw = data.full_redraw.max(1);
            data.state.request_redraw();
        }
        UdevEvent::Changed { device_id } => {
            info!(device_id, "DRM device changed");
            let connector_sync = udev_backend_mut(data)
                .map(|backend| backend.handle_device_changed(device_id))
                .unwrap_or_default();
            apply_connector_sync(data, connector_sync);
            data.state.request_redraw();
        }
        UdevEvent::Removed { device_id } => {
            info!(device_id, "DRM device removed");
            let output_descriptors = if let Some(backend) = udev_backend_mut(data) {
                backend.remove_device(device_id);
                backend.output_descriptors()
            } else {
                Vec::new()
            };
            let dh = data.display.handle();
            data.state.sync_connector_outputs(&dh, output_descriptors);
            data.state.reconcile_pointer_after_output_geometry_change();
            data.full_redraw = data.full_redraw.max(1);
            data.state.request_redraw();
        }
    })?;

    let input_source = Some(insert_libinput_source(
        &handle,
        session.clone(),
        &seat_name,
    )?);

    if let Some(drm_notifier) = drm_notifier {
        handle.insert_source(drm_notifier, |event, metadata, data| match event {
            DrmEvent::VBlank(crtc) => {
                trace!(?crtc, ?metadata, "DRM vblank");
                let submitted_frame = if let Some(backend) = udev_backend_mut(data) {
                    match backend.frame_submitted(crtc) {
                        Ok(result) => result,
                        Err(error) => {
                            error!(%error, "failed to mark native frame submitted");
                            None
                        }
                    }
                } else {
                    None
                };
                if let Some(should_redraw) = submitted_frame {
                    if let Err(error) = data.send_frame_callbacks() {
                        warn!(%error, "failed to send native frame callbacks");
                    }
                    if should_redraw || data.state.viewport_animation.is_some() {
                        data.state.request_redraw();
                    }
                }
            }
            DrmEvent::Error(error) => error!(%error, "DRM event error"),
        })?;
    }

    let backend = UdevBackendState {
        session,
        seat_name,
        session_active,
        loop_handle: handle.clone(),
        input_source,
        kms_devices_active: primary_device.is_some() && session_active,
        drm_commits_paused: !session_active,
        connector_rescan_pending: false,
        repaint_pending: false,
        frame_pending: false,
        frame_dirty: false,
        primary_device,
        devices,
        input_event_count: 0,
        emergency_exit_ctrl_pressed: false,
        emergency_exit_alt_pressed: false,
    };
    backend.log_summary();
    let mut data = super::create_calloop_data(
        app,
        display,
        super::Backend::Udev(Box::new(backend)),
        output_size,
        options.exit_after,
    );
    event_loop.dispatch(Some(Duration::from_millis(0)), &mut data)?;

    info!("native backend initialized; entering compositor event loop");
    super::run_event_loop(event_loop, &mut data)
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
    let dh = data.display.handle();
    if let Some(primary) = connector_sync.primary {
        let size = primary.size;
        data.state.set_primary_output_descriptor(primary);
        data.damage_tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
        data.full_redraw = 1;
        data.state.configure_shell_bars();
    }
    data.state
        .sync_connector_outputs(&dh, connector_sync.descriptors);
    data.state.reconcile_pointer_after_output_geometry_change();
    data.full_redraw = data.full_redraw.max(1);
}

fn create_libinput_backend(
    session: LibSeatSession,
    seat_name: &str,
) -> Result<LibinputInputBackend, Box<dyn std::error::Error>> {
    let mut libinput_context = Libinput::new_with_udev(LibinputSessionInterface::from(session));
    libinput_context
        .udev_assign_seat(seat_name)
        .map_err(|()| format!("failed to assign libinput to seat {seat_name}"))?;
    Ok(LibinputInputBackend::new(libinput_context))
}

fn insert_libinput_source(
    handle: &LoopHandle<'static, super::CalloopData>,
    session: LibSeatSession,
    seat_name: &str,
) -> Result<RegistrationToken, Box<dyn std::error::Error>> {
    let libinput_backend = create_libinput_backend(session, seat_name)?;
    let token = handle.insert_source(libinput_backend, |event, _, data| {
        {
            let Some(backend) = udev_backend_mut(data) else {
                return;
            };
            if !backend.session_active {
                trace!("input event ignored while native session is paused");
                return;
            }
            backend.input_event_count += 1;
            log_input_event(&event);
            if backend.handle_emergency_exit_chord(&event) {
                info!("native emergency exit chord pressed; stopping compositor event loop");
                data.running = false;
                return;
            }
        }

        super::handle_input_event(&mut data.state, event);
    })?;
    info!(seat = seat_name, "native libinput source registered");
    Ok(token)
}

impl UdevBackendState {
    pub(in crate::compositor) fn render_frame(
        &mut self,
        state: &mut super::App,
        damage_tracker: &mut OutputDamageTracker,
        force_full_redraw: bool,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if self.drm_commits_paused {
            debug!("skipping native render while DRM commits are paused");
            self.frame_dirty = true;
            return Ok(false);
        }
        if self.frame_pending {
            self.frame_dirty = true;
            debug!("native redraw requested while page flip is pending; deferring until vblank");
            return Ok(false);
        }

        let Some(device) = self.primary_device.as_mut() else {
            return Err("udev backend has no primary DRM device".into());
        };
        let renderer = &mut device.render_node.renderer;
        let Some(gbm_surface) = device
            .output_surface
            .as_mut()
            .map(|output| &mut output.gbm_surface)
        else {
            debug!("skipping native render because no KMS output surface is initialized");
            self.frame_dirty = true;
            return Ok(false);
        };

        let (mut dmabuf, buffer_age) = gbm_surface.next_buffer()?;
        let age = if force_full_redraw {
            0
        } else {
            usize::from(buffer_age)
        };
        let damage = {
            let mut framebuffer = renderer.bind(&mut dmabuf)?;
            state.render_frame(renderer, &mut framebuffer, damage_tracker, age)?
        };
        // Keep damage tracking for renderer-side redraw minimization, but do not
        // forward damage clips to KMS yet. Some virtual DRM stacks reject
        // FB_DAMAGE_CLIPS on page flip with EINVAL under client redraw load.
        let _damage = damage;
        gbm_surface.queue_buffer(None, None, ())?;
        self.frame_pending = true;
        self.frame_dirty = false;
        self.repaint_pending = true;
        trace!(crtc = ?gbm_surface.crtc(), age, "queued native frame");
        Ok(true)
    }

    pub(in crate::compositor) fn frame_submitted(
        &mut self,
        crtc: crtc::Handle,
    ) -> Result<Option<bool>, Box<dyn std::error::Error>> {
        let Some(gbm_surface) = self
            .primary_device
            .as_mut()
            .and_then(|device| device.output_surface.as_mut())
            .map(|output| &mut output.gbm_surface)
        else {
            return Ok(None);
        };
        if gbm_surface.crtc() != crtc {
            return Ok(None);
        }
        gbm_surface.frame_submitted()?;
        self.frame_pending = false;
        self.repaint_pending = false;
        let should_redraw = self.frame_dirty;
        if should_redraw {
            debug!("native page flip completed with dirty state; scheduling next frame");
        }
        Ok(Some(should_redraw))
    }

    pub(in crate::compositor) fn import_dmabuf(
        &mut self,
        dmabuf: &Dmabuf,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Some(renderer) = self
            .primary_device
            .as_mut()
            .map(|device| &mut device.render_node.renderer)
        else {
            return Err("udev renderer is not initialized".into());
        };
        renderer.import_dmabuf(dmabuf, None).map_err(|error| {
            format!(
                "failed to import native dmabuf size={:?} format={:?} planes={} has_modifier={} node={:?}: {error}",
                dmabuf.size(),
                dmabuf.format(),
                dmabuf.num_planes(),
                dmabuf.has_modifier(),
                dmabuf.node()
            )
        })?;
        Ok(())
    }

    fn pause_session(&mut self) {
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
            device.output_surface = None;
            device.active = false;
        }
        info!("native session paused; DRM commits disabled and Wayland clients remain connected");
    }

    fn activate_session(&mut self) -> ConnectorSync {
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
        let next_target = device.output_targets.first().cloned();
        let primary = next_target.as_ref().map(KmsOutputTarget::output_descriptor);
        if let Err(error) = device.rebuild_output_surface(next_target) {
            error!(%error, "failed to rebuild primary KMS output surface after session activation");
        }

        self.kms_devices_active = device.output_surface.is_some();
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
            descriptors: device.output_descriptors(),
            primary,
        }
    }

    fn add_or_update_device(&mut self, device_id: u64, path: PathBuf) {
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

    fn output_descriptors(&self) -> Vec<super::OutputDescriptor> {
        self.primary_device
            .as_ref()
            .map(UdevDevice::output_descriptors)
            .unwrap_or_default()
    }

    fn handle_device_changed(&mut self, device_id: u64) -> ConnectorSync {
        self.connector_rescan_pending = true;
        if !self.session_active || self.drm_commits_paused {
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
                descriptors: device.output_descriptors(),
                primary: None,
            };
        }

        let previous_target = device.output_target.clone();
        let next_targets = device.connected_output_targets();
        let next_target = next_targets.first().cloned();
        if previous_target == next_target {
            debug!("primary DRM connector state re-scanned; selected output is unchanged");
            self.connector_rescan_pending = false;
            device.output_targets = next_targets;
            return ConnectorSync {
                descriptors: device.output_descriptors(),
                primary: None,
            };
        }

        info!(previous_target = ?previous_target, next_target = ?next_target, "primary DRM connector selection changed; rebuilding output surface");
        device.output_targets = next_targets;
        let primary = next_target.as_ref().map(KmsOutputTarget::output_descriptor);
        if let Err(error) = device.rebuild_output_surface(next_target) {
            error!(%error, "failed to rebuild primary KMS output surface");
        }

        self.kms_devices_active = self.session_active && device.output_surface.is_some();
        self.drm_commits_paused = !self.session_active || device.output_surface.is_none();
        self.connector_rescan_pending = false;
        self.frame_pending = false;
        self.frame_dirty = self.kms_devices_active;
        self.repaint_pending = self.kms_devices_active;

        ConnectorSync {
            descriptors: device.output_descriptors(),
            primary,
        }
    }

    fn remove_device(&mut self, device_id: u64) {
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

    fn log_summary(&self) {
        info!(
            session_active = self.session_active,
            kms_devices_active = self.kms_devices_active,
            drm_commits_paused = self.drm_commits_paused,
            connector_rescan_pending = self.connector_rescan_pending,
            repaint_pending = self.repaint_pending,
            drm_device_count = self.devices.len(),
            input_event_count = self.input_event_count,
            "native backend state"
        );
        if let Some(device) = &self.primary_device {
            if let Some(output) = &device.output_surface {
                info!(
                    path = %device.path.display(),
                    connector = ?output.target.connector,
                    crtc = ?output.target.crtc,
                    mode = ?output.target.mode,
                    gbm_surface = true,
                    "primary DRM device opened through session"
                );
            } else {
                info!(path = %device.path.display(), gbm_surface = false, "primary DRM device opened through session");
            }
        }
    }
}
