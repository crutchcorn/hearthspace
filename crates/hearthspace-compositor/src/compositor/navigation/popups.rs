use smithay::{
    backend::input::InputTime,
    utils::{IsAlive, SERIAL_COUNTER},
};

use crate::compositor::App;

impl App {
    pub(in crate::compositor) fn cleanup_popups(&mut self) {
        self.popups.cleanup();
        if !self
            .popup_grab
            .as_ref()
            .is_some_and(|grab| grab.has_ended())
        {
            return;
        }
        let grab = self.popup_grab.take().unwrap();
        let serial = grab.serial();
        let previous = grab.previous_serial().unwrap_or(serial);
        let keyboard = self.keyboard.clone();
        let pointer = self.pointer.clone();
        let owned_keyboard = keyboard.has_grab(serial) || keyboard.has_grab(previous);
        let owned_pointer = pointer.has_grab(serial) || pointer.has_grab(previous);
        // Smithay's default popup grabs normally retire on the next input
        // event. Retire completed grabs here as well so a menu closed by its
        // client does not block shortcuts and agent actions until mouse motion.
        if owned_pointer {
            pointer.unset_grab(self, SERIAL_COUNTER.next_serial(), InputTime::now());
        }
        if owned_keyboard {
            // Unsetting the pointer grab may already have unset this keyboard
            // grab; preserve any replacement grab established in its callback.
            if keyboard.has_grab(serial) || keyboard.has_grab(previous) {
                keyboard.unset_grab(self);
            }
            if !keyboard.is_grabbed() {
                let root = grab.current_grab().filter(IsAlive::alive);
                keyboard.set_focus(self, root.clone(), SERIAL_COUNTER.next_serial());
                if root.is_none() {
                    self.restore_window_focus();
                }
            }
        }
    }
}
