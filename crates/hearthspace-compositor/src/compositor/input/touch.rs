//! Route direct-touch recognition into shared viewport and pointer operations.
use smithay::{
    backend::input::{ButtonState, InputTime, TouchSlot},
    utils::{Logical, Point},
};

use super::{super::App, pinch::PinchInput, remote::PointerSource, touch_tracker::TouchAction};
use crate::compositor::switcher::SwitcherSource;

impl App {
    pub(in crate::compositor) fn touch_down(
        &mut self,
        device: &str,
        slot: TouchSlot,
        location: Point<f64, Logical>,
        time: InputTime,
    ) {
        self.gtk_shell.reset_input();
        self.shortcuts.cancel_meta_tap();
        let first = !self.touch_input.has_contacts();
        if first && !self.group_touch_blocked() {
            self.dismiss_group_outside_press(location, true);
        }
        if first {
            self.cancel_shell_popup_outside_press(location);
        }
        if self.favorite_touch_down(device, slot, location) {
            return;
        }
        if first {
            self.focused_shell_pointer(location, true);
        }
        if !first && self.touch_pointer_pressed {
            self.cancel_layout_drag();
            self.cancel_titlebar_pointer();
            self.cancel_desktop_gesture();
        }
        if first {
            // A previous sequence can have lifted in this same input frame.
            // Deliver its queued end/release before starting the next one.
            let actions = self.touch_input.frame_all();
            self.apply_touch_actions(actions, time);
            if !self.window_switcher_active() {
                self.interrupt_pinch_zoom();
            }
        }
        self.begin_ui_touch(device, slot, location, time, first);
        if !first && self.window_switcher_active() {
            self.invalidate_window_switcher_tap();
        }
        self.touch_input.down(device, slot, location);
        // Canvas previews accept ordinary single-contact taps. An extra
        // contact cancels the pending tap without activating the revealed app.
        if ((self.canvas_search_active()
            || self.window_switcher_source() == Some(SwitcherSource::Keyboard)
            || self.layout_editing())
            && !first)
            || (first
                && !self.canvas_search_active()
                && !self.window_switcher_active()
                && (self.group_touch_suppresses_gestures() || self.pinch_blocked())
                && !self.session_locked())
        {
            self.cancel_canvas_search_touch();
            if self.window_switcher_active() {
                self.cancel_window_switcher_touch();
            }
            let actions = self.touch_input.interrupt();
            self.apply_touch_actions(actions, time);
        }
    }

    pub(in crate::compositor) fn touch_motion(
        &mut self,
        device: &str,
        slot: TouchSlot,
        location: Point<f64, Logical>,
        time: InputTime,
    ) {
        if !self.favorite_touch_motion(device, slot, location) {
            self.motion_ui_touch(device, slot, location, time);
            self.touch_input.motion(device, slot, location);
        }
    }

    pub(in crate::compositor) fn touch_up(
        &mut self,
        device: &str,
        slot: TouchSlot,
        time: InputTime,
    ) {
        if !self.favorite_touch_up(device, slot, false) {
            self.end_ui_touch(device, slot, false, time);
            self.touch_input.up(device, slot);
        }
    }

    pub(in crate::compositor) fn touch_cancel(
        &mut self,
        device: &str,
        slot: TouchSlot,
        time: InputTime,
    ) {
        if self.touch_pointer_pressed && self.touch_input.has_device_input(device) {
            self.cancel_titlebar_pointer();
            self.cancel_canvas_search_touch();
            if self.window_switcher_active() {
                self.cancel_window_switcher_touch();
            }
        }
        self.favorite_touch_up(device, slot, true);
        self.end_ui_touch(device, slot, true, time);
        self.touch_input.cancel(device, slot);
        self.flush_touch_frame(device, time);
    }

    pub(in crate::compositor) fn flush_touch_frame(&mut self, device: &str, time: InputTime) {
        let actions = self.touch_input.frame(device);
        self.apply_touch_actions(actions, time);
    }

    pub(in crate::compositor) fn remove_touch_device(&mut self, device: &str) {
        // A completed tap may outlive the tracker's contacts while GTK replies.
        self.gtk_shell.reset_input();
        if self.touch_pointer_pressed && self.touch_input.has_device_input(device) {
            self.cancel_titlebar_pointer();
            self.cancel_desktop_gesture();
            self.cancel_layout_drag();
            self.cancel_canvas_search_touch();
            if self.window_switcher_active() {
                self.cancel_window_switcher_touch();
            }
        }
        self.end_ui_touch(device, None.into(), true, InputTime::now());
        let actions = self.touch_input.remove_device(device);
        self.apply_touch_actions(actions, InputTime::now());
    }

    pub(in crate::compositor) fn cancel_input_gestures(&mut self) {
        self.gtk_shell.reset_input();
        self.cancel_titlebar_pointer();
        self.cancel_group_gesture();
        self.cancel_desktop_gesture();
        self.cancel_layout_drag();
        self.cancel_canvas_search_touch();
        if self.window_switcher_active() {
            self.cancel_window_switcher_touch();
        }
        self.shortcuts.cancel_meta_tap();
        self.cancel_ui_touch();
        self.cancel_pending_search();
        let actions = self.touch_input.reset();
        self.apply_touch_actions(actions, InputTime::now());
        self.cancel_pinch_zoom();
    }

    pub(super) fn apply_touch_actions(
        &mut self,
        actions: Vec<(String, TouchAction)>,
        time: InputTime,
    ) {
        let was_dispatching = self.dispatching_touch;
        self.dispatching_touch = true;
        for (device, action) in actions {
            if self.suppress_ui_touch_pointer(&device, &action) {
                continue;
            }
            match action {
                TouchAction::Hover(location) | TouchAction::Motion(location) => {
                    self.apply_pointer_motion(location, time);
                }
                TouchAction::Press(origin) => {
                    self.apply_pointer_motion(origin, time);
                    self.touch_pointer_button(ButtonState::Pressed);
                }
                TouchAction::Release => {
                    self.touch_pointer_button(ButtonState::Released);
                }
                TouchAction::Tap(location) => {
                    self.apply_pointer_motion(location, time);
                    self.touch_pointer_button(ButtonState::Pressed);
                    self.touch_pointer_button(ButtonState::Released);
                }
                TouchAction::BeginPinch { anchor } => {
                    self.start_pinch_zoom(PinchInput::Touchscreen, device, anchor);
                }
                TouchAction::UpdatePinch { scale, translation } => {
                    self.update_pinch_zoom(
                        PinchInput::Touchscreen,
                        &device,
                        scale,
                        translation,
                        time,
                    );
                }
                TouchAction::EndPinch { cancelled } => {
                    self.end_pinch_zoom(PinchInput::Touchscreen, &device, cancelled, time);
                }
                TouchAction::BeginSwitcherPinch { fingers } => {
                    self.start_switcher_pinch(PinchInput::Touchscreen, device, fingers);
                }
                TouchAction::UpdateSwitcherPinch { scale } => {
                    self.update_switcher_pinch(PinchInput::Touchscreen, &device, scale);
                }
                TouchAction::EndSwitcherPinch { cancelled } => {
                    self.end_switcher_pinch(PinchInput::Touchscreen, &device, cancelled);
                }
                TouchAction::BeginSwitcher { fingers } => {
                    self.start_window_switcher_swipe(PinchInput::Touchscreen, device, fingers);
                }
                TouchAction::UpdateSwitcher { translation } => {
                    self.update_window_switcher_swipe(
                        PinchInput::Touchscreen,
                        &device,
                        translation,
                    );
                }
                TouchAction::EndSwitcher { cancelled } => {
                    self.end_window_switcher_swipe(PinchInput::Touchscreen, &device, cancelled);
                }
            }
        }
        self.dispatching_touch = was_dispatching;
        self.finish_ui_touch_frame();
    }

    pub(super) fn touch_pointer_button(&mut self, state: ButtonState) {
        // Cleanup may discard a queued press. Do not release another input
        // source's button unless this touchscreen actually delivered its press.
        if state == ButtonState::Released && !self.touch_pointer_pressed {
            return;
        }
        self.touch_pointer_pressed = state == ButtonState::Pressed;
        self.handle_pointer_button_from_source(
            PointerSource::Touch,
            0x110,
            state,
            InputTime::now(),
        );
    }
}
