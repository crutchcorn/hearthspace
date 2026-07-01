use smithay::backend::{
    input::{InputEvent, KeyState, KeyboardKeyEvent},
    libinput::LibinputInputBackend,
};
use tracing::{debug, trace};

use super::UdevBackendState;

impl UdevBackendState {
    pub(super) fn handle_emergency_exit_chord(
        &mut self,
        event: &InputEvent<LibinputInputBackend>,
    ) -> bool {
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
}

pub(super) fn log_input_event(event: &InputEvent<LibinputInputBackend>) {
    match event {
        InputEvent::DeviceAdded { device } => debug!(name = device.name(), "input device added"),
        InputEvent::DeviceRemoved { device } => {
            debug!(name = device.name(), "input device removed");
        }
        InputEvent::Keyboard { .. } => trace!("input keyboard event"),
        InputEvent::PointerMotion { .. } => trace!("input relative pointer motion event"),
        InputEvent::PointerMotionAbsolute { .. } => trace!("input absolute pointer motion event"),
        InputEvent::PointerButton { .. } => trace!("input pointer button event"),
        InputEvent::PointerAxis { .. } => trace!("input pointer axis event"),
        InputEvent::GestureSwipeBegin { .. }
        | InputEvent::GestureSwipeUpdate { .. }
        | InputEvent::GestureSwipeEnd { .. }
        | InputEvent::GesturePinchBegin { .. }
        | InputEvent::GesturePinchUpdate { .. }
        | InputEvent::GesturePinchEnd { .. }
        | InputEvent::GestureHoldBegin { .. }
        | InputEvent::GestureHoldEnd { .. } => {
            debug!("input gesture event ignored until native compositor state is wired");
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
