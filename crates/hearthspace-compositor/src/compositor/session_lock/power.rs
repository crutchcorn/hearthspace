//! Sleep preparation shields immediately, before starting the authentication UI.
use smithay::{backend::input::InputTime, input::pointer::MotionEvent, utils::SERIAL_COUNTER};

use crate::compositor::App;

impl App {
    pub(in crate::compositor) fn prepare_for_sleep(&mut self) -> Result<(), String> {
        if !self.session_lock.supported {
            return Err("sleep preparation requires a lock-capable backend".into());
        }
        if !self.session_locked() {
            self.arm_session_shield();
            self.session_lock.awaiting_locker = true;
        }
        if self.session_lock.awaiting_locker {
            // A missing/crashed executable must never reveal the desktop.
            self.launch_session_locker()?;
        }
        Ok(())
    }

    pub(super) fn arm_session_shield(&mut self) {
        self.reset_session_input();
        self.session_lock.previous_focus = self.focused_normal_window_id;
        self.clear_keyboard_focus();
        self.pointer.clone().motion(
            self,
            None,
            &MotionEvent {
                location: self.pointer_location,
                serial: SERIAL_COUNTER.next_serial(),
                time: InputTime::now(),
            },
        );
        self.pointer.clone().frame(self);
        self.cursor_surface = None;
        self.cursor_icon = crate::compositor::cursor::CursorIcon::Default;
        self.cursor_visible = false;
        let outputs = self.capture_outputs().into_iter().map(|output| output.name);
        self.session_lock.barrier.begin(outputs);
        self.request_redraw();
    }

    pub(in crate::compositor) fn invalidate_lock_presentation(&mut self) {
        self.session_lock.barrier.invalidate();
        self.request_redraw();
    }
}
