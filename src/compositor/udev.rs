use std::{io, path::PathBuf, time::Duration};

use smithay::{
    backend::{
        allocator::dmabuf::Dmabuf,
        allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        allocator::{Buffer, Format, Fourcc},
        drm::{DrmDevice, DrmDeviceFd, DrmEvent, GbmBufferedSurface},
        egl::{EGLContext, EGLDisplay},
        input::{InputEvent, KeyState, KeyboardKeyEvent},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{Bind, ImportDma, damage::OutputDamageTracker, gles::GlesRenderer},
        session::{Event as SessionEvent, Session, libseat::LibSeatSession},
        udev::{UdevBackend, UdevEvent, all_gpus, primary_gpu},
    },
    output::{PhysicalProperties, Subpixel},
    reexports::drm::control::{Device as ControlDevice, Mode, connector, crtc},
    reexports::{
        input::Libinput,
        rustix::fs::{OFlags, stat},
        wayland_server::Display,
    },
    utils::{DeviceFd, Physical, Size},
};

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

struct UdevDevice {
    path: PathBuf,
    render_node: RenderNode,
    scanout_node: ScanoutNode,
    active: bool,
    output_targets: Vec<KmsOutputTarget>,
    output_target: Option<KmsOutputTarget>,
    output_surface: Option<KmsOutputSurface>,
}

struct RenderNode {
    drm_fd: DrmDeviceFd,
    renderer: GlesRenderer,
}

struct ScanoutNode {
    drm_fd: DrmDeviceFd,
    drm_device: DrmDevice,
}

struct KmsOutputSurface {
    target: KmsOutputTarget,
    gbm_surface: GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, ()>,
}

#[derive(Debug, Clone, PartialEq)]
struct KmsOutputTarget {
    connector: connector::Handle,
    crtc: crtc::Handle,
    mode: Mode,
    connector_name: String,
    physical_size_mm: (i32, i32),
}

struct UdevDeviceInfo {
    device_id: u64,
    path: PathBuf,
}

impl KmsOutputTarget {
    fn output_size(&self) -> Size<i32, Physical> {
        let (width, height) = self.mode.size();
        Size::from((i32::from(width), i32::from(height)))
    }

    fn output_descriptor(&self) -> super::OutputDescriptor {
        super::OutputDescriptor {
            name: self.connector_name.clone(),
            properties: PhysicalProperties {
                size: self.physical_size_mm.into(),
                subpixel: Subpixel::Unknown,
                make: "DRM".into(),
                model: self.connector_name.clone(),
            },
            size: self.output_size(),
            scale: 1,
            refresh: self.refresh_millihz(),
        }
    }

    fn refresh_millihz(&self) -> i32 {
        let refresh_hz = self.mode.vrefresh();
        if refresh_hz == 0 {
            return 60_000;
        }
        i32::try_from(refresh_hz.saturating_mul(1000)).unwrap_or(60_000)
    }
}

pub fn run_udev(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    let termination_signals = super::create_termination_signals()?;
    let (mut session, session_notifier) = LibSeatSession::new()?;
    let seat_name = session.seat();
    println!("Hearthspace native backend acquired seat {seat_name}");

    let udev_backend = UdevBackend::new(&seat_name)?;
    let devices = initial_device_list(&udev_backend);
    log_device_list(&seat_name, &devices);

    let primary_device = match primary_gpu(&seat_name)? {
        Some(path) => {
            println!("Selected primary DRM device {}", path.display());
            for device in devices.iter().filter(|device| device.path != path) {
                println!(
                    "Ignoring secondary DRM device {} at {} for now",
                    device.device_id,
                    device.path.display()
                );
            }
            let active = session.is_active();
            let (device, notifier) = create_udev_device(&mut session, path, active)?;
            Some((device, notifier))
        }
        None => {
            println!("No primary DRM device available for seat {seat_name}");
            None
        }
    };

    let (primary_device, drm_notifier) = match primary_device {
        Some((device, notifier)) => (Some(device), Some(notifier)),
        None => (None, None),
    };

    let session_active = session.is_active();
    let mut libinput_context =
        Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
    libinput_context
        .udev_assign_seat(&seat_name)
        .map_err(|()| format!("failed to assign libinput to seat {seat_name}"))?;
    let libinput_backend = LibinputInputBackend::new(libinput_context);

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
                println!(
                    "Native dmabuf feedback main device is {} from {}",
                    main_device,
                    device.path.display()
                );
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

    handle.insert_source(session_notifier, |event, _, data| match event {
        SessionEvent::PauseSession => {
            if let Some(backend) = udev_backend_mut(data) {
                backend.pause_session();
            }
        }
        SessionEvent::ActivateSession => {
            if let Some(backend) = udev_backend_mut(data) {
                backend.activate_session();
            }
            data.full_redraw = data.full_redraw.max(1);
            data.state.request_redraw();
        }
    })?;

    handle.insert_source(udev_backend, |event, _, data| match event {
        UdevEvent::Added { device_id, path } => {
            println!("DRM device added {device_id} at {}", path.display());
            let output_descriptors = if let Some(backend) = udev_backend_mut(data) {
                backend.add_or_update_device(device_id as u64, path);
                backend.output_descriptors()
            } else {
                Vec::new()
            };
            let dh = data.display.handle();
            data.state.sync_connector_outputs(&dh, output_descriptors);
            data.full_redraw = data.full_redraw.max(1);
            data.state.request_redraw();
        }
        UdevEvent::Changed { device_id } => {
            println!("DRM device changed {device_id}");
            let output_descriptors = udev_backend_mut(data)
                .map(|backend| backend.handle_device_changed(device_id as u64))
                .unwrap_or_default();
            let dh = data.display.handle();
            data.state.sync_connector_outputs(&dh, output_descriptors);
            data.full_redraw = data.full_redraw.max(1);
            data.state.request_redraw();
        }
        UdevEvent::Removed { device_id } => {
            println!("DRM device removed {device_id}");
            let output_descriptors = if let Some(backend) = udev_backend_mut(data) {
                backend.remove_device(device_id as u64);
                backend.output_descriptors()
            } else {
                Vec::new()
            };
            let dh = data.display.handle();
            data.state.sync_connector_outputs(&dh, output_descriptors);
            data.full_redraw = data.full_redraw.max(1);
            data.state.request_redraw();
        }
    })?;

    handle.insert_source(libinput_backend, |event, _, data| {
        {
            let Some(backend) = udev_backend_mut(data) else {
                return;
            };
            if !backend.session_active {
                println!("Input event ignored while native session is paused");
                return;
            }
            backend.input_event_count += 1;
            log_input_event(&event);
            if backend.handle_emergency_exit_chord(&event) {
                println!("Native emergency exit chord pressed; stopping compositor event loop");
                data.running = false;
                return;
            }
        }

        super::handle_input_event(&mut data.state, event);
    })?;

    if let Some(drm_notifier) = drm_notifier {
        handle.insert_source(drm_notifier, |event, metadata, data| match event {
            DrmEvent::VBlank(crtc) => {
                println!("DRM vblank for {:?} with metadata {:?}", crtc, metadata);
                let submitted_frame = if let Some(backend) = udev_backend_mut(data) {
                    match backend.frame_submitted(crtc) {
                        Ok(result) => result,
                        Err(error) => {
                            eprintln!("Failed to mark native frame submitted: {error}");
                            None
                        }
                    }
                } else {
                    None
                };
                if let Some(should_redraw) = submitted_frame {
                    if let Err(error) = data.send_frame_callbacks() {
                        eprintln!("Failed to send native frame callbacks: {error}");
                    }
                    if should_redraw || data.state.viewport_animation.is_some() {
                        data.state.request_redraw();
                    }
                }
            }
            DrmEvent::Error(error) => eprintln!("DRM event error: {error}"),
        })?;
    }

    let backend = UdevBackendState {
        session,
        seat_name,
        session_active,
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

    println!("Native backend initialized; entering compositor event loop");
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

impl UdevBackendState {
    fn handle_emergency_exit_chord(&mut self, event: &InputEvent<LibinputInputBackend>) -> bool {
        const KEY_ESC: u32 = 1 + 8;
        const KEY_BACKSPACE: u32 = 14 + 8;
        const KEY_LEFTCTRL: u32 = 29 + 8;
        const KEY_LEFTALT: u32 = 56 + 8;
        const KEY_RIGHTCTRL: u32 = 97 + 8;
        const KEY_RIGHTALT: u32 = 100 + 8;

        let InputEvent::Keyboard { event } = event else {
            return false;
        };
        let keycode: u32 = event.key_code().into();
        let pressed = event.state() == KeyState::Pressed;
        match keycode {
            KEY_LEFTCTRL | KEY_RIGHTCTRL => self.emergency_exit_ctrl_pressed = pressed,
            KEY_LEFTALT | KEY_RIGHTALT => self.emergency_exit_alt_pressed = pressed,
            KEY_BACKSPACE | KEY_ESC if pressed => {
                return self.emergency_exit_ctrl_pressed && self.emergency_exit_alt_pressed;
            }
            _ => {}
        }
        false
    }

    pub(in crate::compositor) fn render_frame(
        &mut self,
        state: &mut super::App,
        damage_tracker: &mut OutputDamageTracker,
        force_full_redraw: bool,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if self.drm_commits_paused {
            println!("Skipping native render while DRM commits are paused");
            self.frame_dirty = true;
            return Ok(false);
        }
        if self.frame_pending {
            self.frame_dirty = true;
            println!("Native redraw requested while page flip is pending; deferring until vblank");
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
            return Err("udev GBM surface is not initialized".into());
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
        gbm_surface.queue_buffer(None, damage, ())?;
        self.frame_pending = true;
        self.frame_dirty = false;
        self.repaint_pending = true;
        println!(
            "Queued native frame on CRTC {:?} with buffer age {}",
            gbm_surface.crtc(),
            age
        );
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
            println!("Native page flip completed with dirty state; scheduling next frame");
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
        if let Some(device) = &mut self.primary_device {
            device.active = false;
        }
        println!(
            "Native session paused; DRM commits disabled and Wayland clients remain connected"
        );
    }

    fn activate_session(&mut self) {
        self.session_active = true;
        self.drm_commits_paused = false;
        if let Some(device) = &mut self.primary_device {
            device.active = true;
        } else if let Err(error) = self.open_primary_device() {
            eprintln!("Failed to open primary DRM device after session activation: {error}");
        }

        self.kms_devices_active = self.primary_device.is_some();
        self.connector_rescan_pending = true;
        self.repaint_pending = self.kms_devices_active;
        self.frame_dirty = self.kms_devices_active;

        if let Err(error) = self.rescan_devices() {
            eprintln!("Failed to re-scan DRM devices after session activation: {error}");
        }

        println!(
            "Native session activated; DRM devices reactivated, connector rescan queued, repaint queued: {}",
            self.repaint_pending
        );
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

    fn handle_device_changed(&mut self, device_id: u64) -> Vec<super::OutputDescriptor> {
        self.connector_rescan_pending = true;
        if let Err(error) = self.rescan_devices() {
            eprintln!("Failed to re-scan DRM devices after device change: {error}");
        }

        let Some(device) = self.primary_device.as_mut() else {
            return Vec::new();
        };
        if device.scanout_node.drm_fd.dev_id().ok().map(|id| id as u64) != Some(device_id) {
            println!("Changed DRM device {device_id} is not the selected primary device");
            return device.output_descriptors();
        }

        let previous_target = device.output_target.clone();
        let next_targets = device.connected_output_targets();
        let next_target = next_targets.first().cloned();
        if previous_target == next_target {
            println!("Primary DRM connector state re-scanned; selected output is unchanged");
            self.connector_rescan_pending = false;
            device.output_targets = next_targets;
            return device.output_descriptors();
        }

        println!(
            "Primary DRM connector selection changed from {:?} to {:?}; output rebuild remains pending",
            previous_target, next_target
        );
        device.output_targets = next_targets;
        device.output_target = next_target;
        self.frame_dirty = true;
        self.repaint_pending = true;
        device.output_descriptors()
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
            println!("Primary DRM device removed; KMS state marked inactive");
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
        println!(
            "Native backend session active: {}; KMS active: {}; DRM commits paused: {}; connector rescan pending: {}; repaint pending: {}; {} DRM device(s) known; {} input event(s) processed",
            self.session_active,
            self.kms_devices_active,
            self.drm_commits_paused,
            self.connector_rescan_pending,
            self.repaint_pending,
            self.devices.len(),
            self.input_event_count
        );
        if let Some(device) = &self.primary_device {
            let target = device
                .output_surface
                .as_ref()
                .map(|output| {
                    format!(
                        "; selected connector {:?}, CRTC {:?}, mode {:?}",
                        output.target.connector, output.target.crtc, output.target.mode
                    )
                })
                .unwrap_or_default();
            println!(
                "Primary DRM device opened through session: {}{}; gbm_surface={}",
                device.path.display(),
                target,
                device.output_surface.is_some(),
            );
        }
    }
}

impl UdevDevice {
    fn output_descriptors(&self) -> Vec<super::OutputDescriptor> {
        self.output_targets
            .iter()
            .map(KmsOutputTarget::output_descriptor)
            .collect()
    }

    fn connected_output_targets(&self) -> Vec<KmsOutputTarget> {
        let Ok(resources) = self.scanout_node.drm_device.resource_handles() else {
            eprintln!(
                "Failed to query DRM resource handles for {}",
                self.path.display()
            );
            return Vec::new();
        };

        let crtcs = resources.crtcs();
        println!(
            "DRM device {} exposes {} connector(s) and {} CRTC(s)",
            self.path.display(),
            resources.connectors().len(),
            crtcs.len()
        );

        let mut targets = Vec::new();
        for connector_handle in resources.connectors() {
            let Ok(info) = self
                .scanout_node
                .drm_device
                .get_connector(*connector_handle, true)
            else {
                eprintln!("Failed to query DRM connector {:?}", connector_handle);
                continue;
            };
            let connected = info.state() == connector::State::Connected;
            println!(
                "DRM connector {:?} {:?} connected={} mode_count={}",
                connector_handle,
                info.interface(),
                connected,
                info.modes().len()
            );
            if connected {
                if let Some(mode) = info.modes().first() {
                    println!(
                        "Preferred/first mode for {:?}: {:?}",
                        connector_handle, mode
                    );
                    let crtc = info
                        .current_encoder()
                        .and_then(|encoder| self.scanout_node.drm_device.get_encoder(encoder).ok())
                        .and_then(|encoder| encoder.crtc())
                        .or_else(|| {
                            info.encoders().iter().find_map(|encoder| {
                                self.scanout_node
                                    .drm_device
                                    .get_encoder(*encoder)
                                    .ok()
                                    .and_then(|encoder_info| {
                                        resources
                                            .filter_crtcs(encoder_info.possible_crtcs())
                                            .into_iter()
                                            .next()
                                    })
                            })
                        });
                    if let Some(crtc) = crtc {
                        let physical_size_mm = info
                            .size()
                            .map(|(width, height)| {
                                (
                                    i32::try_from(width).unwrap_or(0),
                                    i32::try_from(height).unwrap_or(0),
                                )
                            })
                            .unwrap_or((0, 0));
                        targets.push(KmsOutputTarget {
                            connector: *connector_handle,
                            crtc,
                            mode: *mode,
                            connector_name: format!(
                                "{}-{}",
                                info.interface().as_str(),
                                info.interface_id()
                            ),
                            physical_size_mm,
                        });
                    }
                }
                println!("CRTC candidates for {:?}: {:?}", connector_handle, crtcs);
            }
        }
        if let Some(target) = targets.first() {
            println!(
                "Selected KMS target connector {:?}, CRTC {:?}, mode {:?}",
                target.connector, target.crtc, target.mode
            );
        } else {
            println!("No connected DRM connector with a usable CRTC was found");
        }
        targets
    }
}

fn create_udev_device(
    session: &mut LibSeatSession,
    path: PathBuf,
    active: bool,
) -> Result<(UdevDevice, smithay::backend::drm::DrmDeviceNotifier), Box<dyn std::error::Error>> {
    let fd = session.open(&path, OFlags::RDWR | OFlags::CLOEXEC)?;
    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let gbm_device = GbmDevice::new(drm_fd.clone())?;
    let egl_display = unsafe { EGLDisplay::new(gbm_device.clone())? };
    let context = EGLContext::new(&egl_display)?;
    let renderer = unsafe { GlesRenderer::new(context)? };
    let gbm_allocator = GbmAllocator::new(
        gbm_device,
        GbmBufferFlags::SCANOUT | GbmBufferFlags::RENDERING,
    );
    // Avoid disabling existing connectors until the first real frame commit path is in place.
    let (drm_device, notifier) = DrmDevice::new(drm_fd.clone(), false)?;
    let mut device = UdevDevice {
        path,
        render_node: RenderNode {
            drm_fd: drm_fd.clone(),
            renderer,
        },
        scanout_node: ScanoutNode { drm_fd, drm_device },
        active,
        output_targets: Vec::new(),
        output_target: None,
        output_surface: None,
    };
    device.output_targets = device.connected_output_targets();
    device.output_target = device.output_targets.first().cloned();
    if let Some(target) = device.output_target.clone() {
        let surface = device.scanout_node.drm_device.create_surface(
            target.crtc,
            target.mode,
            std::slice::from_ref(&target.connector),
        )?;
        println!(
            "Created DRM surface for connector {:?}, CRTC {:?}, mode {:?}",
            target.connector, target.crtc, target.mode
        );
        let renderer_formats = device
            .render_node
            .renderer
            .dmabuf_formats()
            .into_iter()
            .collect::<Vec<Format>>();
        let gbm_surface = GbmBufferedSurface::new(
            surface,
            gbm_allocator,
            &[Fourcc::Argb8888, Fourcc::Xrgb8888],
            renderer_formats,
        )?;
        println!(
            "Created GBM buffered surface for connector {:?}, CRTC {:?}",
            target.connector, target.crtc
        );
        device.output_surface = Some(KmsOutputSurface {
            target,
            gbm_surface,
        });
    }
    Ok((device, notifier))
}

fn initial_device_list(udev_backend: &UdevBackend) -> Vec<UdevDeviceInfo> {
    udev_backend
        .device_list()
        .map(|(device_id, path)| UdevDeviceInfo {
            device_id: device_id as u64,
            path: path.to_path_buf(),
        })
        .collect()
}

fn current_device_list(seat_name: &str) -> io::Result<Vec<UdevDeviceInfo>> {
    all_gpus(seat_name)?
        .into_iter()
        .map(|path| {
            let device_id = stat(&path)?.st_rdev as u64;
            Ok(UdevDeviceInfo { device_id, path })
        })
        .collect()
}

fn log_device_list(seat_name: &str, devices: &[UdevDeviceInfo]) {
    if devices.is_empty() {
        println!("No DRM devices found for seat {seat_name}");
        return;
    }

    for device in devices {
        println!(
            "Found DRM device {} at {}",
            device.device_id,
            device.path.display()
        );
    }
}

fn log_input_event(event: &InputEvent<LibinputInputBackend>) {
    match event {
        InputEvent::DeviceAdded { device } => println!("Input device added: {}", device.name()),
        InputEvent::DeviceRemoved { device } => {
            println!("Input device removed: {}", device.name());
        }
        InputEvent::Keyboard { .. } => println!("Input keyboard event"),
        InputEvent::PointerMotion { .. } => println!("Input relative pointer motion event"),
        InputEvent::PointerMotionAbsolute { .. } => println!("Input absolute pointer motion event"),
        InputEvent::PointerButton { .. } => println!("Input pointer button event"),
        InputEvent::PointerAxis { .. } => println!("Input pointer axis event"),
        InputEvent::GestureSwipeBegin { .. }
        | InputEvent::GestureSwipeUpdate { .. }
        | InputEvent::GestureSwipeEnd { .. }
        | InputEvent::GesturePinchBegin { .. }
        | InputEvent::GesturePinchUpdate { .. }
        | InputEvent::GesturePinchEnd { .. }
        | InputEvent::GestureHoldBegin { .. }
        | InputEvent::GestureHoldEnd { .. } => {
            println!("Input gesture event ignored until native compositor state is wired");
        }
        InputEvent::TouchDown { .. }
        | InputEvent::TouchMotion { .. }
        | InputEvent::TouchUp { .. }
        | InputEvent::TouchCancel { .. }
        | InputEvent::TouchFrame { .. } => {
            println!("Input touch event ignored until native touch handling is needed");
        }
        InputEvent::TabletToolAxis { .. }
        | InputEvent::TabletToolProximity { .. }
        | InputEvent::TabletToolTip { .. }
        | InputEvent::TabletToolButton { .. } => {
            println!("Input tablet event ignored until native tablet handling is needed");
        }
        InputEvent::SwitchToggle { .. } => {
            println!("Input switch event ignored until native switch handling is needed");
        }
        InputEvent::Special(_) => println!("Backend-specific input event ignored"),
    }
}
