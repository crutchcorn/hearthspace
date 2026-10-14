//! Compositor-owned two-finger zoom; Smithay/libinput recognize the gesture.
use smithay::{
    backend::input::InputTime,
    utils::{Logical, Point},
};
use tracing::{debug, trace};

use crate::geometry::{CanvasPoint, zoom_around_screen_point};

use super::super::App;

#[derive(Default)]
pub(in crate::compositor) struct PinchZoom {
    gesture: Option<Gesture>,
}

struct Gesture {
    device: String,
    offset: CanvasPoint,
    scale: f64,
    anchor: Point<f64, Logical>,
}

impl PinchZoom {
    pub(in crate::compositor) fn is_active(&self) -> bool {
        self.gesture.is_some()
    }

    fn begin(
        &mut self,
        device: String,
        offset: CanvasPoint,
        scale: f64,
        anchor: Point<f64, Logical>,
    ) {
        self.gesture = Some(Gesture {
            device,
            offset,
            scale,
            anchor,
        });
    }

    fn update(&self, device: &str, scale: f64) -> Option<(CanvasPoint, f64)> {
        let gesture = self.gesture.as_ref()?;
        if gesture.device != device || !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        // The backend scale is absolute since begin, not a per-event multiplier.
        // Always use that snapshot, including at the limits, to avoid drift.
        Some(zoom_around_screen_point(
            gesture.offset,
            gesture.scale,
            gesture.anchor,
            scale,
        ))
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
        self.gesture
            .take()
            .map(|gesture| (gesture.offset, gesture.scale))
    }

    fn interrupt(&mut self) {
        self.gesture = None;
    }
}

impl App {
    fn pinch_blocked(&self) -> bool {
        self.drag.is_some()
            || self.resize.is_some()
            || self.pointer.is_grabbed()
            || self.keyboard.is_grabbed()
            || self.active_touch_slot.is_some()
    }

    pub(super) fn begin_pinch_zoom(&mut self, device: String, fingers: u32) {
        if fingers != 2 || self.pinch_zoom.is_active() || self.pinch_blocked() {
            return;
        }
        self.advance_viewport_animation();
        self.viewport_animation = None;
        debug!(%device, anchor = ?self.pointer_location, "starting canvas pinch zoom");
        self.pinch_zoom.begin(
            device,
            self.viewport_offset,
            self.viewport_scale,
            self.pointer_location,
        );
        self.apply_pointer_motion(self.pointer_location, InputTime::now());
        self.request_redraw();
    }

    pub(super) fn update_pinch_zoom(&mut self, device: &str, scale: f64, time: InputTime) {
        if self.pinch_blocked() {
            // A newly opened popup or drag takes ownership of the interaction.
            // Keep its currently displayed transform; stale updates cannot resume.
            self.interrupt_pinch_zoom();
            return;
        }
        if let Some((offset, scale)) = self.pinch_zoom.update(device, scale) {
            trace!(%device, scale, "updating canvas pinch zoom");
            self.apply_pinch_viewport(offset, scale, time);
        }
    }

    pub(super) fn end_pinch_zoom(&mut self, device: &str, cancelled: bool, time: InputTime) {
        debug!(
            %device,
            cancelled,
            active = self.pinch_zoom.is_active(),
            blocked = self.pinch_blocked(),
            "ending canvas pinch zoom"
        );
        if self.pinch_blocked() {
            self.interrupt_pinch_zoom();
            return;
        }
        if let Some((offset, scale)) = self.pinch_zoom.end(device, cancelled) {
            self.apply_pinch_viewport(offset, scale, time);
        }
    }

    pub(in crate::compositor) fn cancel_pinch_zoom(&mut self) {
        if self.pinch_blocked() {
            self.interrupt_pinch_zoom();
            return;
        }
        if let Some((offset, scale)) = self.pinch_zoom.cancel() {
            self.apply_pinch_viewport(offset, scale, InputTime::now());
        }
    }

    pub(in crate::compositor) fn interrupt_pinch_zoom(&mut self) {
        self.pinch_zoom.interrupt();
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
mod tests;
