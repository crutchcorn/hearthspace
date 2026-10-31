use std::{collections::BTreeMap, time::Duration};

use smithay::{
    backend::{
        session::{Event as SessionEvent, Session, libseat::LibSeatSession},
        udev::{UdevBackend, UdevEvent, primary_gpu},
    },
    reexports::wayland_server::Display,
    utils::{Physical, Size},
};
use tracing::info;

use super::{
    PublishedOutputs, UdevBackendState, apply_connector_sync,
    device::{initial_device_list, log_device_list},
    gpu::GpuState,
    identity::ConnectorNames,
    native_shortcuts::NativeShortcuts,
    recovery, udev_backend_mut,
};
use crate::{
    RunOptions,
    config::{HEADLESS_OUTPUT_HEIGHT, HEADLESS_OUTPUT_WIDTH},
};

pub fn run_udev(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    let termination_signals = crate::compositor::create_termination_signals()?;
    let (session, session_notifier) = LibSeatSession::new()?;
    let seat_name = session.seat();
    let udev_backend = UdevBackend::new(&seat_name)?;
    let mut devices = initial_device_list(&udev_backend);
    let preferred = primary_gpu(&seat_name)?;
    devices.sort_by_key(|device| (Some(&device.path) != preferred.as_ref(), device.device_id));
    log_device_list(&seat_name, &devices);
    let session_active = session.is_active();
    let display: Display<crate::compositor::App> = Display::new()?;
    let dh = display.handle();
    let output_size = Size::<i32, Physical>::from((HEADLESS_OUTPUT_WIDTH, HEADLESS_OUTPUT_HEIGHT));
    let primary_output = crate::compositor::create_output(&dh, output_size, 1);
    let dmabuf = crate::compositor::create_dmabuf_global(&dh, Vec::new(), None)?;
    let crate::compositor::AppInit {
        display,
        mut event_loop,
        handle,
        app,
    } = crate::compositor::initialize_app(
        display,
        options,
        primary_output,
        dmabuf,
        termination_signals,
    )?;

    handle.insert_source(session_notifier, |event, _, data| match event {
        SessionEvent::PauseSession => {
            data.state.set_native_lock_outputs_available(false);
            data.state.set_remote_input_active(false);
            data.state.remote_capture.stop();
            data.state.reset_session_input();
            data.state.invalidate_lock_presentation();
            data.captures.invalidate("native session became inactive");
            if let Some(backend) = udev_backend_mut(data) {
                backend.pause_session();
            }
        }
        SessionEvent::ActivateSession => recovery::activate_session(data),
    })?;

    handle.insert_source(udev_backend, |event, _, data| {
        let sync = match event {
            UdevEvent::Added { device_id, path } => {
                info!(device_id, path = %path.display(), "DRM device added");
                udev_backend_mut(data)
                    .map(|backend| {
                        backend.add_or_update_device(device_id, path);
                        backend.handle_device_changed(device_id)
                    })
                    .unwrap_or_default()
            }
            UdevEvent::Changed { device_id } => udev_backend_mut(data)
                .map(|backend| backend.handle_device_changed(device_id))
                .unwrap_or_default(),
            UdevEvent::Removed { device_id } => {
                info!(device_id, "DRM device removed");
                udev_backend_mut(data)
                    .map(|backend| {
                        backend.remove_device(device_id);
                        backend.connector_sync()
                    })
                    .unwrap_or_default()
            }
        };
        apply_connector_sync(data, sync);
        recovery::schedule_recovery(data);
    })?;

    let mut backend = UdevBackendState {
        session,
        seat_name,
        session_active,
        loop_handle: handle.clone(),
        input_source: None,
        recovery_source: None,
        bridge_poll_source: None,
        bridge_poll_failed: false,
        recovery_progress_at: None,
        recovery_needed: true,
        recovery_exhausted: false,
        opened_devices: BTreeMap::new(),
        devices,
        gpus: GpuState::new()?,
        connector_names: ConnectorNames::default(),
        published_outputs: PublishedOutputs::default(),
        next_incarnation: 0,
        kms_devices_active: false,
        drm_commits_paused: true,
        connector_rescan_pending: true,
        input_event_count: 0,
        native_shortcuts: NativeShortcuts::default(),
    };
    // Open in preferred-GPU order before any clients or input are dispatched.
    backend.open_missing_devices();
    let mut data = crate::compositor::create_calloop_data(
        app,
        display,
        crate::compositor::Backend::Udev(Box::new(backend)),
        output_size,
        options.exit_after,
    );
    if session_active {
        recovery::activate_session(&mut data);
    } else {
        let sync = udev_backend_mut(&mut data).unwrap().connector_sync();
        apply_connector_sync(&mut data, sync);
    }
    if let Some(backend) = udev_backend_mut(&mut data) {
        backend.log_summary();
    }
    event_loop.dispatch(Some(Duration::from_millis(0)), &mut data)?;
    crate::compositor::run_event_loop(event_loop, &mut data)
}
