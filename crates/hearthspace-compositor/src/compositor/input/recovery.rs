//! Releases that happen while the seat is inactive will never reach libinput.
//! End interactions before detaching devices so no grab or modifier survives.
use smithay::{
    backend::input::{ButtonState, InputTime, KeyState},
    desktop::PopupUngrabStrategy,
    input::pointer::ButtonEvent,
    utils::SERIAL_COUNTER,
};

use crate::compositor::App;

impl App {
    pub(in crate::compositor) fn deliver_pointer_button(&mut self, event: &ButtonEvent) {
        let owned_release =
            event.state == ButtonState::Released && self.gtk_pointer_release_owned(event.button);
        match event.state {
            ButtonState::Pressed => self.pressed_pointer_buttons.insert(event.button),
            ButtonState::Released => self.pressed_pointer_buttons.remove(&event.button),
        };
        self.pointer.clone().button(self, event);
        self.record_gtk_pointer_button(event, owned_release);
    }

    pub(in crate::compositor) fn reset_session_input(&mut self) {
        // Touch/remote cleanup or direct releases below can drop WindowGrab.
        // Preserve its deferred handoff until every input grab is reset.
        let owner_retired = self.window_pointer_grab_owner_retired();
        self.reset_interaction_modes();
        let time = InputTime::now();
        self.reset_remote_input();
        // Deliver releases to the existing focus/grab before dismissing it.
        // Shortcut releases use the normal filter so intercepted presses cannot
        // leak an unmatched release to the client.
        for key in self.keyboard.pressed_keys() {
            self.handle_keyboard_input(key, KeyState::Released, time);
        }
        for button in self.pressed_pointer_buttons.clone() {
            self.deliver_pointer_button(&ButtonEvent {
                serial: SERIAL_COUNTER.next_serial(),
                time,
                button,
                state: ButtonState::Released,
            });
        }
        self.clear_pointer_sources();
        // Source reset also discards mouse buttons swallowed during a native
        // touch move; no late release may keep that retired capture alive.
        self.ui_touch = None;
        self.gtk_shell.reset_input();
        self.reset_titlebar_input();
        self.pointer.clone().frame(self);
        if let Some(mut grab) = self.popup_grab.take() {
            grab.ungrab(PopupUngrabStrategy::All);
        }
        self.pointer
            .clone()
            .unset_grab(self, SERIAL_COUNTER.next_serial(), time);
        self.keyboard.clone().unset_grab(self);
        self.restore_retired_window_grab_focus(owner_retired);
        self.request_redraw();
    }
}
