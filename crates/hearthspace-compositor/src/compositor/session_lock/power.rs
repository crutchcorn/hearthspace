//! Sleep preparation shields immediately, before starting the authentication UI.
use smithay::{backend::input::InputTime, input::pointer::MotionEvent, utils::SERIAL_COUNTER};

use crate::compositor::{App, CalloopData, runtime::Backend, shell_integration::ControlReply};

impl App {
    pub(in crate::compositor) fn prepare_for_sleep(&mut self) -> Result<(), String> {
        if !self.session_lock.supported {
            return Err("sleep preparation requires a lock-capable backend".into());
        }
        // Arm even when already locked, and before a fallible locker launch.
        self.session_lock.sleep_prepared = true;
        self.lock_shield()
    }

    pub(in crate::compositor) fn lock_shield(&mut self) -> Result<(), String> {
        if !self.session_lock.supported {
            return Err("lock shielding requires a lock-capable backend".into());
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

    fn finish_sleep_preparation(&mut self) -> Result<(), String> {
        let was_prepared = std::mem::take(&mut self.session_lock.sleep_prepared);
        if was_prepared && self.session_lock.awaiting_locker {
            // Only an explicit authenticated unlock (or an initial missing
            // locker) permits replacement. A dead owning client stays closed.
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

impl CalloopData {
    pub(in crate::compositor) fn resume_session(&mut self) -> ControlReply {
        if !self.state.session_lock.supported {
            return ControlReply::Err(
                "session-resume requires the native or headless backend".into(),
            );
        }
        let locker = self.state.finish_sleep_preparation();
        // Recover display/input even if starting a replacement locker fails.
        // The opaque shield remains and the launch error is reported afterward.
        match &self.backend {
            #[cfg(feature = "udev")]
            Backend::Udev(_) => {
                if let ControlReply::Err(error) = self.resume_native_session() {
                    return ControlReply::Err(error);
                }
            }
            Backend::Headless(_) => {
                self.state.reset_session_input();
                self.state.invalidate_lock_presentation();
            }
            #[cfg(feature = "winit")]
            Backend::Winit(_) => unreachable!("nested sessions cannot support locking"),
        }
        locker.map_or_else(ControlReply::Err, |_| ControlReply::Ok)
    }
}
