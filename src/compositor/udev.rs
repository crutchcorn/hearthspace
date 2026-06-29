use std::{io, os::fd::OwnedFd, path::PathBuf, time::Duration};

use smithay::{
    backend::{
        input::InputEvent,
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        session::{Event as SessionEvent, Session, libseat::LibSeatSession},
        udev::{UdevBackend, UdevEvent, all_gpus, primary_gpu},
    },
    reexports::{
        input::Libinput,
        rustix::fs::{OFlags, stat},
        wayland_server::Display,
    },
    utils::{Physical, Size},
};

use crate::{
    RunOptions,
    config::{HEADLESS_OUTPUT_HEIGHT, HEADLESS_OUTPUT_WIDTH},
};

pub(super) struct UdevBackendState {
    session: LibSeatSession,
    seat_name: String,
    session_active: bool,
    primary_device: Option<UdevDevice>,
    devices: Vec<UdevDeviceInfo>,
    kms_devices_active: bool,
    drm_commits_paused: bool,
    connector_rescan_pending: bool,
    repaint_pending: bool,
    input_event_count: u64,
}

struct UdevDevice {
    path: PathBuf,
    _fd: OwnedFd,
    active: bool,
}

struct UdevDeviceInfo {
    device_id: u64,
    path: PathBuf,
}

pub fn run_udev(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    let (mut session, session_notifier) = LibSeatSession::new()?;
    let seat_name = session.seat();
    println!("Hearthspace native backend acquired seat {seat_name}");

    if options.start_shell {
        println!("Native shell startup is deferred until KMS modesetting is implemented");
    }

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
            let fd = session.open(&path, OFlags::RDWR | OFlags::CLOEXEC)?;
            Some(UdevDevice {
                path,
                _fd: fd,
                active: session.is_active(),
            })
        }
        None => {
            println!("No primary DRM device available for seat {seat_name}");
            None
        }
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
    let output_size = Size::<i32, Physical>::from((HEADLESS_OUTPUT_WIDTH, HEADLESS_OUTPUT_HEIGHT));
    let output = super::create_output(&dh, output_size, 1);
    let dmabuf = super::create_dmabuf_global(&dh, Vec::new(), None)?;
    let app_options = RunOptions {
        start_shell: false,
        ..options
    };
    let super::AppInit {
        display,
        mut event_loop,
        handle,
        app,
    } = super::initialize_app(display, app_options, output_size, output, dmabuf)?;

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
        }
    })?;

    handle.insert_source(udev_backend, |event, _, data| match event {
        UdevEvent::Added { device_id, path } => {
            println!("DRM device added {device_id} at {}", path.display());
            if let Some(backend) = udev_backend_mut(data) {
                backend.add_or_update_device(device_id as u64, path);
            }
        }
        UdevEvent::Changed { device_id } => {
            println!("DRM device changed {device_id}");
            if let Some(backend) = udev_backend_mut(data) {
                backend.connector_rescan_pending = true;
            }
        }
        UdevEvent::Removed { device_id } => {
            println!("DRM device removed {device_id}");
            if let Some(backend) = udev_backend_mut(data) {
                backend.remove_device(device_id as u64);
            }
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
        }

        super::handle_input_event(&mut data.state, event);
    })?;

    let backend = UdevBackendState {
        session,
        seat_name,
        session_active,
        kms_devices_active: primary_device.is_some() && session_active,
        drm_commits_paused: !session_active,
        connector_rescan_pending: false,
        repaint_pending: false,
        primary_device,
        devices,
        input_event_count: 0,
    };
    backend.log_summary();
    let mut data = super::create_calloop_data(
        app,
        display,
        super::Backend::Udev(Box::new(backend)),
        output_size,
    );
    event_loop.dispatch(Some(Duration::from_millis(0)), &mut data)?;

    if let super::Backend::Udev(backend) = &data.backend {
        backend.log_summary();
    }

    println!("Native backend skeleton initialized; KMS modesetting is not implemented yet");
    Ok(())
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
    fn pause_session(&mut self) {
        self.session_active = false;
        self.drm_commits_paused = true;
        self.kms_devices_active = false;
        self.repaint_pending = false;
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
        let fd = self.session.open(&path, OFlags::RDWR | OFlags::CLOEXEC)?;
        self.primary_device = Some(UdevDevice {
            path,
            _fd: fd,
            active: self.session_active,
        });
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
            println!(
                "Primary DRM device opened through session: {}",
                device.path.display()
            );
        }
    }
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
