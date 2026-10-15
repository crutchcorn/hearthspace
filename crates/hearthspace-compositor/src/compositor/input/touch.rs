//! Route direct-touch recognition into shared viewport and pointer operations.
use smithay::{
    backend::input::{ButtonState, InputTime, TouchSlot},
    utils::{Logical, Point},
};

use super::{super::App, pinch::PinchInput, touch_tracker::TouchAction};

impl App {
    pub(super) fn touch_down(
        &mut self,
        device: &str,
        slot: TouchSlot,
        location: Point<f64, Logical>,
        time: InputTime,
    ) {
        self.shortcuts.cancel_meta_tap();
        if self.favorite_touch_down(device, slot, location) {
            return;
        }
        let first = !self.touch_input.has_contacts();
        if first {
            // A previous sequence can have lifted in this same input frame.
            // Deliver its queued end/release before starting the next one.
            let actions = self.touch_input.frame_all();
            self.apply_touch_actions(actions, time);
            if !self.window_switcher_active() {
                self.interrupt_pinch_zoom();
            }
        }
        self.touch_input.down(device, slot, location);
        // An existing switcher owns the whole chord. Suppress even a pending
        // tap so lifting after Alt is released cannot click the revealed app.
        if first
            && (self.pinch_blocked() || self.window_switcher_active())
            && !self.session_locked()
        {
            let actions = self.touch_input.interrupt();
            self.apply_touch_actions(actions, time);
        }
    }

    pub(super) fn flush_touch_frame(&mut self, device: &str, time: InputTime) {
        let actions = self.touch_input.frame(device);
        self.apply_touch_actions(actions, time);
    }

    pub(super) fn remove_touch_device(&mut self, device: &str) {
        let actions = self.touch_input.remove_device(device);
        self.apply_touch_actions(actions, InputTime::now());
    }

    pub(in crate::compositor) fn cancel_input_gestures(&mut self) {
        self.shortcuts.cancel_meta_tap();
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
                TouchAction::BeginOverview { fingers } => {
                    self.start_overview_pinch(PinchInput::Touchscreen, device, fingers);
                }
                TouchAction::UpdateOverview { scale } => {
                    self.update_overview_pinch(PinchInput::Touchscreen, &device, scale);
                }
                TouchAction::EndOverview { cancelled } => {
                    self.end_overview_pinch(PinchInput::Touchscreen, &device, cancelled);
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
    }

    fn touch_pointer_button(&mut self, state: ButtonState) {
        // Cleanup may discard a queued press. Do not release another input
        // source's button unless this touchscreen actually delivered its press.
        if state == ButtonState::Released && !self.touch_pointer_pressed {
            return;
        }
        self.touch_pointer_pressed = state == ButtonState::Pressed;
        self.synthesize_pointer_button(0x110, state);
    }
}
