use std::{os::fd::OwnedFd, path::PathBuf, time::Duration};

use smithay::{
    backend::{
        input::InputEvent,
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        session::{Event as SessionEvent, Session, libseat::LibSeatSession},
        udev::{UdevBackend, UdevEvent, primary_gpu},
    },
    reexports::{calloop::EventLoop, input::Libinput, rustix::fs::OFlags},
};

use crate::RunOptions;

pub(super) struct UdevBackendState {
    session_active: bool,
    primary_device: Option<UdevDevice>,
    devices: Vec<UdevDeviceInfo>,
    input_event_count: u64,
}

struct UdevDevice {
    path: PathBuf,
    _fd: OwnedFd,
}

struct UdevDeviceInfo {
    device_id: u64,
    path: PathBuf,
}

pub fn run_udev(_options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    let (mut session, session_notifier) = LibSeatSession::new()?;
    let seat_name = session.seat();
    println!("Hearthspace native backend acquired seat {seat_name}");

    let udev_backend = UdevBackend::new(&seat_name)?;
    let devices = udev_backend
        .device_list()
        .map(|(device_id, path)| UdevDeviceInfo {
            device_id: device_id as u64,
            path: path.to_path_buf(),
        })
        .collect::<Vec<_>>();
    if devices.is_empty() {
        println!("No DRM devices found for seat {seat_name}");
    } else {
        for device in &devices {
            println!(
                "Found DRM device {} at {}",
                device.device_id,
                device.path.display()
            );
        }
    }

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
            Some(UdevDevice { path, _fd: fd })
        }
        None => {
            println!("No primary DRM device available for seat {seat_name}");
            None
        }
    };

    let session_active = session.is_active();
    let mut libinput_context = Libinput::new_with_udev(LibinputSessionInterface::from(session));
    libinput_context
        .udev_assign_seat(&seat_name)
        .map_err(|()| format!("failed to assign libinput to seat {seat_name}"))?;
    let libinput_backend = LibinputInputBackend::new(libinput_context);

    let mut event_loop = EventLoop::<UdevBackendState>::try_new()?;
    let handle = event_loop.handle();

    handle.insert_source(session_notifier, |event, _, data| match event {
        SessionEvent::PauseSession => {
            data.session_active = false;
            println!("Native session paused");
        }
        SessionEvent::ActivateSession => {
            data.session_active = true;
            println!("Native session activated");
        }
    })?;

    handle.insert_source(udev_backend, |event, _, _data| match event {
        UdevEvent::Added { device_id, path } => {
            println!("DRM device added {device_id} at {}", path.display());
        }
        UdevEvent::Changed { device_id } => {
            println!("DRM device changed {device_id}");
        }
        UdevEvent::Removed { device_id } => {
            println!("DRM device removed {device_id}");
        }
    })?;

    handle.insert_source(libinput_backend, |event, _, data| {
        data.input_event_count += 1;
        log_input_event(&event);
    })?;

    let mut data = UdevBackendState {
        session_active,
        primary_device,
        devices,
        input_event_count: 0,
    };
    data.log_summary();
    event_loop.dispatch(Some(Duration::from_millis(0)), &mut data)?;

    println!("Native backend skeleton initialized; KMS modesetting is not implemented yet");
    Ok(())
}

impl UdevBackendState {
    fn log_summary(&self) {
        println!(
            "Native backend session active: {}; {} DRM device(s) known; {} input event(s) processed",
            self.session_active,
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
        _ => println!("Input event ignored until native compositor state is wired"),
    }
}
