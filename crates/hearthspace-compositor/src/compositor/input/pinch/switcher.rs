//! A source-owned horizontal swipe advances once, then repeats with hysteresis.
use smithay::{
    backend::input::InputTime,
    utils::{Logical, Point},
};

use crate::compositor::switcher::SwitcherSource;

use super::{App, PinchInput, finite_point};

const START_DISTANCE: f64 = 32.0;
const STEP_DISTANCE: f64 = 96.0;
const REVERSE_DISTANCE: f64 = 48.0;

pub(super) struct SwitcherGesture {
    input: PinchInput,
    device: String,
    translation: Point<f64, Logical>,
    checkpoint: f64,
    direction: i32,
    vertical: bool,
}

impl SwitcherGesture {
    fn begin(input: PinchInput, device: String, fingers: u32) -> Option<Self> {
        (matches!(input, PinchInput::TouchpadSwipe | PinchInput::Touchscreen)
            && matches!(fingers, 4 | 5))
        .then_some(Self {
            input,
            device,
            translation: (0.0, 0.0).into(),
            checkpoint: 0.0,
            direction: 0,
            vertical: false,
        })
    }

    fn owns(&self, input: PinchInput, device: &str) -> bool {
        self.input == input && self.device == device
    }

    fn advance(
        &mut self,
        input: PinchInput,
        device: &str,
        translation: Point<f64, Logical>,
    ) -> i32 {
        if !self.owns(input, device)
            || !finite_point(translation)
            || translation.x.abs().max(translation.y.abs()) > 1_000_000.0
        {
            return 0;
        }
        self.translation = translation;
        if self.vertical {
            return 0;
        }
        let mut steps = 0;
        if self.direction == 0 {
            if translation.y.abs() >= START_DISTANCE
                && translation.y.abs() > translation.x.abs() * 1.5
            {
                self.vertical = true;
                return 0;
            }
            if translation.x.abs() < START_DISTANCE
                || translation.x.abs() < translation.y.abs() * 1.5
            {
                return 0;
            }
            self.direction = if translation.x > 0.0 { 1 } else { -1 };
            self.checkpoint = f64::from(self.direction) * START_DISTANCE;
            steps = self.direction;
        }
        // Bound work for an abnormally large backend update. Normal input may
        // cross several detents in one frame without losing its displacement.
        for _ in 0..31 {
            let distance = translation.x - self.checkpoint;
            let direction = f64::from(self.direction);
            if distance * direction >= STEP_DISTANCE {
                self.checkpoint += direction * STEP_DISTANCE;
            } else if distance * direction <= -REVERSE_DISTANCE {
                self.direction = -self.direction;
                self.checkpoint -= direction * REVERSE_DISTANCE;
            } else {
                break;
            }
            steps += self.direction;
        }
        steps
    }
}

impl App {
    pub(in crate::compositor::input) fn start_window_switcher_swipe(
        &mut self,
        input: PinchInput,
        device: String,
        fingers: u32,
    ) {
        if self.pinch_zoom.is_active() || self.pinch_blocked() || self.window_switcher_active() {
            return;
        }
        self.pinch_zoom.switcher = SwitcherGesture::begin(input, device, fingers);
        if self.pinch_zoom.switcher.is_some() {
            self.apply_pointer_motion(self.pointer_location, InputTime::now());
        }
    }

    pub(in crate::compositor::input) fn update_window_switcher_swipe(
        &mut self,
        input: PinchInput,
        device: &str,
        translation: Point<f64, Logical>,
    ) -> bool {
        let Some(gesture) = self.pinch_zoom.switcher.as_mut() else {
            return false;
        };
        let steps = gesture.advance(input, device, translation);
        if self.pinch_blocked() {
            self.interrupt_pinch_zoom();
            return true;
        }
        for _ in 0..steps.unsigned_abs() {
            if self
                .cycle_window_switcher(SwitcherSource::Gesture, steps > 0)
                .is_err()
            {
                self.interrupt_pinch_zoom();
                break;
            }
        }
        true
    }

    pub(super) fn update_touchpad_switcher_swipe(
        &mut self,
        input: PinchInput,
        device: &str,
        delta: Point<f64, Logical>,
    ) -> bool {
        let Some(gesture) = self.pinch_zoom.switcher.as_ref() else {
            return false;
        };
        if input != PinchInput::TouchpadSwipe
            || !gesture.owns(input, device)
            || !finite_point(delta)
        {
            return true;
        }
        self.update_window_switcher_swipe(input, device, gesture.translation + delta)
    }

    pub(in crate::compositor::input) fn end_window_switcher_swipe(
        &mut self,
        input: PinchInput,
        device: &str,
        cancelled: bool,
    ) -> bool {
        let Some(gesture) = self.pinch_zoom.switcher.as_ref() else {
            return false;
        };
        if gesture.owns(input, device) {
            self.pinch_zoom.switcher = None;
            self.finish_window_switcher(SwitcherSource::Gesture, cancelled || self.pinch_blocked());
        }
        true
    }

    pub(in crate::compositor) fn interrupt_window_switcher_gesture(&mut self) {
        if self.pinch_zoom.switcher.take().is_some() && !self.dispatching_touch {
            self.gtk_shell.reset_input();
            let actions = self.touch_input.interrupt();
            self.apply_touch_actions(actions, InputTime::now());
        }
    }

    pub(super) fn cancel_window_switcher_swipe(&mut self) {
        if self.pinch_zoom.switcher.take().is_some() {
            self.finish_window_switcher(SwitcherSource::Gesture, true);
        }
    }
}

#[cfg(test)]
mod tests;
