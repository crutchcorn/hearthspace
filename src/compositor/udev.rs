use std::time::Duration;

use smithay::{
    backend::{
        session::{Event as SessionEvent, Session, libseat::LibSeatSession},
        udev::{UdevBackend, UdevEvent, primary_gpu},
    },
    reexports::calloop::EventLoop,
};

use crate::RunOptions;

struct UdevLoopData {
    session_active: bool,
}

pub fn run_udev(_options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    let (session, session_notifier) = LibSeatSession::new()?;
    let seat_name = session.seat();
    println!("Hearthspace native backend acquired seat {seat_name}");

    let udev_backend = UdevBackend::new(&seat_name)?;
    let devices = udev_backend
        .device_list()
        .map(|(device_id, path)| (device_id, path.to_path_buf()))
        .collect::<Vec<_>>();
    if devices.is_empty() {
        println!("No DRM devices found for seat {seat_name}");
    } else {
        for (device_id, path) in &devices {
            println!("Found DRM device {device_id} at {}", path.display());
        }
    }

    match primary_gpu(&seat_name)? {
        Some(path) => println!("Selected primary DRM device {}", path.display()),
        None => println!("No primary DRM device available for seat {seat_name}"),
    }

    let mut event_loop = EventLoop::<UdevLoopData>::try_new()?;
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

    let mut data = UdevLoopData {
        session_active: session.is_active(),
    };
    event_loop.dispatch(Some(Duration::from_millis(0)), &mut data)?;

    println!("Native backend skeleton initialized; KMS modesetting is not implemented yet");
    Ok(())
}
