use smithay::{
    backend::{
        input::{Device, Event, InputEvent, KeyboardKeyEvent},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        session::{Session, libseat::LibSeatSession},
    },
    reexports::{
        calloop::{LoopHandle, RegistrationToken},
        input::Libinput,
    },
};
use tracing::{debug, info, trace, warn};

use super::{UdevBackendState, native_shortcuts::NativeAction, udev_backend_mut};
use crate::compositor::{CalloopData, handle_input_event};

impl UdevBackendState {
    fn native_shortcut(&mut self, event: &InputEvent<LibinputInputBackend>) -> NativeAction {
        match event {
            InputEvent::Keyboard { event } => self.native_shortcuts.key(
                &event.device().id(),
                event.key_code().into(),
                event.state(),
            ),
            InputEvent::DeviceRemoved { device } => {
                self.native_shortcuts.remove_device(&device.id());
                NativeAction::Forward
            }
            _ => NativeAction::Forward,
        }
    }
}

pub(super) fn insert_libinput_source(
    handle: &LoopHandle<'static, CalloopData>,
    session: LibSeatSession,
    seat_name: &str,
) -> Result<RegistrationToken, Box<dyn std::error::Error>> {
    let mut context = Libinput::new_with_udev(LibinputSessionInterface::from(session));
    context
        .udev_assign_seat(seat_name)
        .map_err(|()| format!("failed to assign libinput to seat {seat_name}"))?;
    let token = handle.insert_source(LibinputInputBackend::new(context), |event, _, data| {
        let Some(backend) = udev_backend_mut(data) else {
            return;
        };
        if !backend.session_active {
            trace!("input event ignored while native session is paused");
            return;
        }
        backend.input_event_count += 1;
        log_input_event(&event);
        // Recovery shortcuts must bypass search's pending-key buffer, shell
        // overlays, client grabs, and the lock screen. Only native events reach
        // this path; nested/headless input cannot switch the host's VT.
        match backend.native_shortcut(&event) {
            NativeAction::Forward => handle_input_event(&mut data.state, event),
            NativeAction::Consume => {}
            NativeAction::SwitchVt(vt) => {
                info!(vt, "native virtual terminal switch requested");
                if let Err(error) = backend.session.change_vt(vt) {
                    warn!(vt, %error, "native virtual terminal switch failed");
                }
            }
            NativeAction::Exit => {
                info!("native emergency exit chord pressed; stopping compositor event loop");
                data.running = false;
            }
        }
    })?;
    info!(seat = seat_name, "native libinput source registered");
    Ok(token)
}

fn log_input_event(event: &InputEvent<LibinputInputBackend>) {
    match event {
        InputEvent::DeviceAdded { device } => {
            debug!(name = ?device.name(), "input device added");
        }
        InputEvent::DeviceRemoved { device } => {
            debug!(name = ?device.name(), "input device removed");
        }
        InputEvent::Keyboard { .. } => trace!("input keyboard event"),
        InputEvent::PointerMotion { .. } => trace!("input relative pointer motion event"),
        InputEvent::PointerMotionAbsolute { .. } => trace!("input absolute pointer motion event"),
        InputEvent::PointerButton { .. } => trace!("input pointer button event"),
        InputEvent::PointerAxis { .. } => trace!("input pointer axis event"),
        InputEvent::GesturePinchBegin { .. }
        | InputEvent::GesturePinchUpdate { .. }
        | InputEvent::GesturePinchEnd { .. } => trace!("input pinch gesture event"),
        InputEvent::GestureSwipeBegin { .. }
        | InputEvent::GestureSwipeUpdate { .. }
        | InputEvent::GestureSwipeEnd { .. } => trace!("input swipe gesture event"),
        InputEvent::GestureHoldBegin { .. } | InputEvent::GestureHoldEnd { .. } => {
            trace!("input hold gesture has no compositor binding");
        }
        InputEvent::TouchDown { .. } => trace!("input touch down event"),
        InputEvent::TouchMotion { .. } => trace!("input touch motion event"),
        InputEvent::TouchUp { .. } => trace!("input touch up event"),
        InputEvent::TouchCancel { .. } => trace!("input touch cancel event"),
        InputEvent::TouchFrame { .. } => trace!("input touch frame event"),
        InputEvent::TabletToolAxis { .. }
        | InputEvent::TabletToolProximity { .. }
        | InputEvent::TabletToolTip { .. }
        | InputEvent::TabletToolButton { .. } => {
            debug!("input tablet event ignored until native tablet handling is needed");
        }
        InputEvent::SwitchToggle { .. } => {
            debug!("input switch event ignored until native switch handling is needed");
        }
        InputEvent::Special(_) => debug!("backend-specific input event ignored"),
    }
}
