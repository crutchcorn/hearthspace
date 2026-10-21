//! Compositor-owned canvas navigation and overview gestures for direct/indirect touch.
use smithay::{
    backend::input::InputTime,
    utils::{Logical, Point},
};
use tracing::{debug, trace};

use crate::geometry::CanvasPoint;

use super::super::{App, presentation::RestoredMaximizedWindow};

mod overview;
mod switcher;
mod transform;
use overview::OverviewGesture;
use switcher::SwitcherGesture;
use transform::{finite_point, gesture_viewport};

#[derive(Default)]
pub(in crate::compositor) struct PinchZoom {
    gesture: Option<Gesture>,
    overview: Option<OverviewGesture>,
    switcher: Option<SwitcherGesture>,
}

struct Gesture {
    input: PinchInput,
    device: String,
    offset: CanvasPoint,
    original_offset: CanvasPoint,
    scale: f64,
    anchor: Point<f64, Logical>,
    translation: Point<f64, Logical>,
    prepared: bool,
    restored: Vec<RestoredMaximizedWindow>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PinchInput {
    Touchpad,
    TouchpadSwipe,
    Touchscreen,
}

impl PinchZoom {
    pub(in crate::compositor) fn is_active(&self) -> bool {
        self.gesture.is_some() || self.overview.is_some() || self.switcher.is_some()
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
            original_offset: offset,
            scale,
            anchor,
            translation: (0.0, 0.0).into(),
            prepared: false,
            restored: Vec::new(),
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

    fn needs_canvas_restore(
        &self,
        device: &str,
        factor: f64,
        translation: Point<f64, Logical>,
    ) -> bool {
        self.gesture.as_ref().is_some_and(|gesture| {
            !gesture.prepared
                && self
                    .update(device, factor, translation)
                    .is_some_and(|viewport| viewport != (gesture.offset, gesture.scale))
        })
    }

    fn prepare_canvas(&mut self, offset: CanvasPoint, restored: Vec<RestoredMaximizedWindow>) {
        if let Some(gesture) = &mut self.gesture {
            gesture.offset = offset;
            gesture.prepared = true;
            gesture.restored = restored;
        }
    }

    fn take_restored(&mut self, device: Option<&str>) -> Vec<RestoredMaximizedWindow> {
        self.gesture
            .as_mut()
            .filter(|gesture| device.is_none_or(|device| device == gesture.device))
            .map(|gesture| std::mem::take(&mut gesture.restored))
            .unwrap_or_default()
    }

    fn end(&mut self, device: &str, cancelled: bool) -> Option<(CanvasPoint, f64)> {
        if self
            .gesture
            .as_ref()
            .is_some_and(|gesture| gesture.device == device)
        {
            let gesture = self.gesture.take().unwrap();
            return cancelled.then_some((gesture.original_offset, gesture.scale));
        }
        None
    }

    fn cancel(&mut self) -> Option<(CanvasPoint, f64)> {
        self.overview = None;
        self.switcher = None;
        self.gesture
            .take()
            .map(|gesture| (gesture.original_offset, gesture.scale))
    }

    fn interrupt_overview(&mut self) -> bool {
        self.overview.take().is_some()
    }

    pub(super) fn interrupt(&mut self) {
        self.overview = None;
        self.switcher = None;
        self.gesture = None;
    }
}

impl App {
    pub(super) fn pinch_blocked(&self) -> bool {
        if self.session_locked() || self.canvas_search_active() {
            return true;
        }
        self.drag.is_some()
            || self.resize.is_some()
            || self.group_interaction_active()
            || self.pointer.is_grabbed()
            || self.keyboard.is_grabbed()
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
            self.start_overview_pinch(input, device, fingers);
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
        if self.update_overview_pinch(input, device, scale) {
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
            || self.pinch_blocked()
            || self.overview_active()
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
        if self.pinch_blocked() {
            // A newly opened popup or drag takes ownership of the interaction.
            // Keep its currently displayed transform; stale updates cannot resume.
            self.interrupt_pinch_zoom();
            return;
        }
        if self
            .pinch_zoom
            .needs_canvas_restore(device, scale, translation)
        {
            // Presentation updates interrupt older input. Hold this gesture
            // locally while releasing maximization, then rebase only its zoom
            // anchor; cancellation still owns the original view and states.
            let mut pinch = std::mem::take(&mut self.pinch_zoom);
            let restored = self.restore_maximized_for_canvas_zoom();
            pinch.prepare_canvas(self.viewport_offset, restored);
            self.pinch_zoom = pinch;
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
            || self.end_overview_pinch(input, device, cancelled)
            || !self.pinch_zoom.accepts(input)
        {
            return;
        }
        debug!(
            %device,
            cancelled,
            active = self.pinch_zoom.is_active(),
            blocked = self.pinch_blocked(),
            "ending three-finger canvas gesture"
        );
        if self.pinch_blocked() {
            self.interrupt_pinch_zoom();
            return;
        }
        let restored = if cancelled {
            self.pinch_zoom.take_restored(Some(device))
        } else {
            Vec::new()
        };
        if let Some((offset, scale)) = self.pinch_zoom.end(device, cancelled) {
            self.cancel_canvas_zoom_restore(restored);
            self.apply_pinch_viewport(offset, scale, time);
        }
    }

    pub(in crate::compositor) fn cancel_pinch_zoom(&mut self) {
        self.cancel_window_switcher_swipe();
        if self.pinch_blocked() {
            self.interrupt_pinch_zoom();
            return;
        }
        let restored = self.pinch_zoom.take_restored(None);
        if let Some((offset, scale)) = self.pinch_zoom.cancel() {
            self.cancel_canvas_zoom_restore(restored);
            self.apply_pinch_viewport(offset, scale, InputTime::now());
        }
    }

    pub(in crate::compositor) fn interrupt_pinch_zoom(&mut self) {
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
