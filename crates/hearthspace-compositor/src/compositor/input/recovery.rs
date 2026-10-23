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
        match event.state {
            ButtonState::Pressed => self.pressed_pointer_buttons.insert(event.button),
            ButtonState::Released => self.pressed_pointer_buttons.remove(&event.button),
        };
        self.pointer.clone().button(self, event);
    }

    pub(in crate::compositor) fn reset_session_input(&mut self) {
        self.clear_canvas_peek_motion();
        self.cancel_pending_shell_click();
        self.cancel_titlebar_pointer();
        self.reset_focused_shell_peek();
        self.reset_window_group_input();
        self.reset_desktop_input();
        self.reset_layout_editor();
        self.reset_layout_pointer_captures();
        self.reset_canvas_search();
        self.reset_favorite_input();
        // Drain queued search modifiers before releasing held keys; otherwise
        // recovery can buffer its own releases or replay a chord on return.
        self.cancel_pending_search();
        self.control_events.clear();
        self.shortcuts.cancel_device_repeat();
        self.dismiss_control_osds();
        self.reset_window_switcher();
        self.close_overview();
        self.overview_input.clear();
        self.cancel_input_gestures();
        self.viewport_animation = None;
        self.drag = None;
        self.finish_resize();
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
        self.reset_titlebar_input();
        self.pointer.clone().frame(self);
        if let Some(mut grab) = self.popup_grab.take() {
            grab.ungrab(PopupUngrabStrategy::All);
        }
        self.pointer
            .clone()
            .unset_grab(self, SERIAL_COUNTER.next_serial(), time);
        self.keyboard.clone().unset_grab(self);
        self.request_redraw();
    }
}
