//! Compositor-owned canvas navigation and switcher pinch gestures for direct/indirect touch.
use smithay::{
    backend::input::InputTime,
    utils::{Logical, Point},
};
use tracing::{debug, trace};

use crate::geometry::CanvasPoint;

use super::super::App;

mod switcher;
mod switcher_pinch;
mod transform;
use switcher::SwitcherGesture;
use switcher_pinch::SwitcherPinch;
use transform::{finite_point, gesture_viewport};

#[derive(Default)]
pub(in crate::compositor) struct PinchZoom {
    gesture: Option<Gesture>,
    switcher_pinch: Option<SwitcherPinch>,
    switcher: Option<SwitcherGesture>,
}

struct Gesture {
    input: PinchInput,
    device: String,
    offset: CanvasPoint,
    scale: f64,
    anchor: Point<f64, Logical>,
    translation: Point<f64, Logical>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PinchInput {
    Touchpad,
    TouchpadSwipe,
    Touchscreen,
}

impl PinchZoom {
    pub(in crate::compositor) fn is_active(&self) -> bool {
        self.gesture.is_some() || self.switcher_pinch.is_some() || self.switcher.is_some()
    }

    fn begin(
        &mut self,
        input: PinchInput,
        device: String,
        offset: CanvasPoint,
        scale: f64,
        anchor: Point<f64, Logical>,
    ) {
        if !finite_point(anchor) || !scale.is_finite() || scale <= 0.0 {
            return;
        }
        self.gesture = Some(Gesture {
            input,
            device,
            offset,
            scale,
            anchor,
            translation: (0.0, 0.0).into(),
        });
    }

    fn accepts(&self, input: PinchInput) -> bool {
        self.gesture
            .as_ref()
            .is_some_and(|gesture| gesture.input == input)
    }

    fn update(
        &self,
        device: &str,
        scale: f64,
        translation: Point<f64, Logical>,
    ) -> Option<(CanvasPoint, f64)> {
        let gesture = self.gesture.as_ref()?;
        if gesture.device != device {
            return None;
        }
        // Both scale and translation are absolute since begin. Recompute from
        // that snapshot and round once, so fractional motion is never lost.
        gesture_viewport(
            gesture.offset,
            gesture.scale,
            gesture.anchor,
            scale,
            translation,
        )
    }

    fn accumulate_translation(
        &mut self,
        input: PinchInput,
        device: &str,
        scale: f64,
        delta: Point<f64, Logical>,
    ) -> Option<Point<f64, Logical>> {
        let gesture = self.gesture.as_mut()?;
        if !matches!(input, PinchInput::Touchpad | PinchInput::TouchpadSwipe)
            || gesture.input != input
            || gesture.device != device
            || !finite_point(delta)
        {
            return None;
        }
        let translation = gesture.translation + delta;
        // Reject the whole malformed update before mutating the accumulator.
        gesture_viewport(
            gesture.offset,
            gesture.scale,
            gesture.anchor,
            scale,
            translation,
        )?;
        gesture.translation = translation;
        Some(translation)
    }

    fn end(&mut self, device: &str, cancelled: bool) -> Option<(CanvasPoint, f64)> {
        if self
            .gesture
            .as_ref()
            .is_some_and(|gesture| gesture.device == device)
        {
            let gesture = self.gesture.take().unwrap();
            return cancelled.then_some((gesture.offset, gesture.scale));
        }
        None
    }

    fn cancel(&mut self) -> Option<(CanvasPoint, f64)> {
        self.switcher_pinch = None;
        self.switcher = None;
        self.gesture
            .take()
            .map(|gesture| (gesture.offset, gesture.scale))
    }

    fn interrupt_switcher_pinch(&mut self) -> bool {
        self.switcher_pinch.take().is_some()
    }

    pub(super) fn interrupt(&mut self) {
        self.switcher_pinch = None;
        self.switcher = None;
        self.gesture = None;
    }
}

impl App {
    pub(super) fn pinch_blocked(&self) -> bool {
        use super::policy::{Action, Phase, Source};
        !self.interaction_allowed(Action::SwitcherGesture, Source::Touch, Phase::Continue)
    }

    fn canvas_pinch_blocked(&self) -> bool {
        use super::policy::{Action, Phase, Source};
        !self.interaction_allowed(Action::CanvasGesture, Source::Touch, Phase::Continue)
    }

    pub(super) fn begin_touchpad_gesture(
        &mut self,
        input: PinchInput,
        device: String,
        fingers: u32,
    ) {
        if !matches!(input, PinchInput::Touchpad | PinchInput::TouchpadSwipe)
            || self.touch_input.has_contacts()
        {
            return;
        }
        if fingers == 3 {
            self.start_pinch_zoom(input, device, self.pointer_location);
        } else if input == PinchInput::TouchpadSwipe {
            self.start_window_switcher_swipe(input, device, fingers);
        } else {
            self.start_switcher_pinch(input, device, fingers);
        }
    }

    pub(super) fn update_touchpad_gesture(
        &mut self,
        input: PinchInput,
        device: &str,
        scale: f64,
        delta: Point<f64, Logical>,
        time: InputTime,
    ) {
        if self.update_touchpad_switcher_swipe(input, device, delta) {
            return;
        }
        if self.update_switcher_pinch(input, device, scale) {
            return;
        }
        if let Some(translation) = self
            .pinch_zoom
            .accumulate_translation(input, device, scale, delta)
        {
            self.update_pinch_zoom(input, device, scale, translation, time);
        }
    }

    pub(super) fn start_pinch_zoom(
        &mut self,
        input: PinchInput,
        device: String,
        anchor: Point<f64, Logical>,
    ) {
        if self.pinch_zoom.is_active()
            || self.canvas_pinch_blocked()
            || self.window_switcher_active()
        {
            return;
        }
        self.advance_viewport_animation();
        self.viewport_animation = None;
        debug!(%device, ?anchor, "starting three-finger canvas gesture");
        self.pinch_zoom.begin(
            input,
            device,
            self.viewport_offset,
            self.viewport_scale,
            anchor,
        );
        self.apply_pointer_motion(self.pointer_location, InputTime::now());
        self.request_redraw();
    }

    pub(super) fn update_pinch_zoom(
        &mut self,
        input: PinchInput,
        device: &str,
        scale: f64,
        translation: Point<f64, Logical>,
        time: InputTime,
    ) {
        if !self.pinch_zoom.accepts(input) {
            return;
        }
        if self.canvas_pinch_blocked() {
            // A newly opened popup or drag takes ownership of the interaction.
            // Keep its currently displayed transform; stale updates cannot resume.
            self.interrupt_pinch_zoom();
            return;
        }
        if let Some((offset, scale)) = self.pinch_zoom.update(device, scale, translation) {
            trace!(%device, scale, ?translation, "updating three-finger canvas gesture");
            self.apply_pinch_viewport(offset, scale, time);
        }
    }

    pub(super) fn end_pinch_zoom(
        &mut self,
        input: PinchInput,
        device: &str,
        cancelled: bool,
        time: InputTime,
    ) {
        if self.end_window_switcher_swipe(input, device, cancelled)
            || self.end_switcher_pinch(input, device, cancelled)
            || !self.pinch_zoom.accepts(input)
        {
            return;
        }
        debug!(
            %device,
            cancelled,
            active = self.pinch_zoom.is_active(),
            blocked = self.canvas_pinch_blocked(),
            "ending three-finger canvas gesture"
        );
        if self.canvas_pinch_blocked() {
            self.interrupt_pinch_zoom();
            return;
        }
        if let Some((offset, scale)) = self.pinch_zoom.end(device, cancelled) {
            self.apply_pinch_viewport(offset, scale, time);
        }
    }

    pub(in crate::compositor) fn cancel_pinch_zoom(&mut self) {
        self.cancel_window_switcher_swipe();
        if self.canvas_pinch_blocked() {
            self.interrupt_pinch_zoom();
            return;
        }
        if let Some((offset, scale)) = self.pinch_zoom.cancel() {
            self.apply_pinch_viewport(offset, scale, InputTime::now());
        }
    }

    pub(in crate::compositor) fn interrupt_pinch_zoom(&mut self) {
        // Interrupting touch can synthesize a release; it cannot complete a click.
        self.gtk_shell.reset_input();
        self.cancel_window_switcher_swipe();
        self.pinch_zoom.interrupt();
        if !self.dispatching_touch {
            let actions = self.touch_input.interrupt();
            self.apply_touch_actions(actions, InputTime::now());
        }
    }

    fn apply_pinch_viewport(&mut self, offset: CanvasPoint, scale: f64, time: InputTime) {
        if self.viewport_offset == offset && self.viewport_scale == scale {
            return;
        }
        self.viewport_offset = offset;
        self.viewport_scale = scale;
        self.viewport_animation = None;
        // A transformed window can move under a stationary pointer. Re-hit-test
        // before the next click so clients receive the correct focus/coordinates.
        self.apply_pointer_motion(self.pointer_location, time);
        self.request_redraw();
    }
}

#[cfg(test)]
mod pan_tests;
#[cfg(test)]
mod tests;
