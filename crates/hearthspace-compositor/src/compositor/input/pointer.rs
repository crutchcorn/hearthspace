use smithay::{
    backend::input::{Axis, ButtonState, InputTime, KeyState},
    input::pointer::{AxisFrame, MotionEvent},
    utils::{Logical, Point, SERIAL_COUNTER},
};
use tracing::{debug, trace};

use crate::compositor::{
    App, HitTarget, cursor::CursorIcon, idle::ActivityReason, windows::resize_cursor_icon,
};

mod constraints;
pub(super) use constraints::relative_pointer_location;
use constraints::{
    clamp_point_to_outputs, constrain_pointer_to_outputs, primary_output_center,
    rect_index_containing_point,
};

impl App {
    pub(in crate::compositor) fn synthesize_key(&mut self, evdev_keycode: u32, state: KeyState) {
        trace!(evdev_keycode, ?state, "synthesizing key input");
        self.handle_keyboard_input(
            evdev_keycode.saturating_add(8).into(),
            state,
            InputTime::now(),
        );
    }

    pub(in crate::compositor) fn synthesize_pointer_motion_abs(
        &mut self,
        location: Point<f64, Logical>,
    ) {
        trace!(?location, "synthesizing absolute pointer motion");
        self.apply_pointer_motion(location, InputTime::now());
    }

    pub(in crate::compositor) fn synthesize_pointer_motion_rel(
        &mut self,
        delta: Point<f64, Logical>,
    ) {
        trace!(?delta, "synthesizing relative pointer motion");
        self.apply_pointer_motion(
            relative_pointer_location(self.raw_pointer_location, delta),
            InputTime::now(),
        );
    }

    pub(in crate::compositor) fn synthesize_pointer_button(
        &mut self,
        button: u32,
        state: ButtonState,
    ) {
        self.handle_pointer_button(button, state, InputTime::now());
    }

    pub(in crate::compositor) fn synthesize_axis(&mut self, horizontal: f64, vertical: f64) {
        trace!(horizontal, vertical, "synthesizing pointer axis");
        if self.zoom_from_scroll_amount(Some(vertical)) {
            return;
        }
        if let Some(HitTarget::Client { window_index, .. }) = self.hit_test(self.pointer_location) {
            self.record_client_activity_for_window_index(window_index, ActivityReason::ClientInput);
        }

        let time = InputTime::now();
        let mut frame = AxisFrame::new(time);
        frame = add_synthetic_axis_to_frame(frame, Axis::Horizontal, horizontal);
        frame = add_synthetic_axis_to_frame(frame, Axis::Vertical, vertical);

        let pointer = self.pointer.clone();
        pointer.axis(self, frame);
        pointer.frame(self);
    }

    pub(in crate::compositor) fn apply_pointer_motion(
        &mut self,
        location: Point<f64, Logical>,
        time: InputTime,
    ) {
        let previous_location = self.pointer_location;
        let output_rects = self.output_logical_rects();
        let previous_output = rect_index_containing_point(previous_location, &output_rects);
        let proposed_output = rect_index_containing_point(location, &output_rects);
        let constrained = constrain_pointer_to_outputs(previous_location, location, &output_rects);
        let visible_output = rect_index_containing_point(constrained.visible, &output_rects);
        if constrained.visible != location || previous_output != visible_output {
            debug!(
                ?previous_location,
                proposed = ?location,
                visible = ?constrained.visible,
                raw = ?constrained.raw,
                ?previous_output,
                ?proposed_output,
                ?visible_output,
                ?output_rects,
                "resolved pointer motion against active output bounds"
            );
        }
        self.pointer_location = constrained.visible;
        self.raw_pointer_location = constrained.raw;
        if self.software_cursor_visible && self.pointer_location != previous_location {
            self.request_redraw();
        }
        if self.titlebar_pointer_motion() {
            self.cursor_surface = None;
            return;
        }

        if self.favorite_pointer_motion() {
            return;
        }

        if self.session_locked() {
            self.lock_pointer_motion(time);
            return;
        }
        if self.overview_pointer_motion(time) {
            return;
        }
        if self.group_pointer_motion(time) {
            return;
        }
        if self.desktop_pointer_motion() {
            return;
        }
        if self.update_window_pointer_operation() {
            return;
        }

        let hit = self.hit_test(self.pointer_location);
        // A client only owns the cursor while the pointer is over its content;
        // over compositor chrome, resize borders, or the canvas the compositor's
        // named cursor takes over, so any lingering client cursor surface is
        // dropped here.
        if !matches!(hit, Some(HitTarget::Client { .. })) {
            self.cursor_surface = None;
        }
        self.cursor_icon = match &hit {
            Some(HitTarget::ResizeBorder { edges, .. }) => resize_cursor_icon(*edges),
            _ => CursorIcon::Default,
        };
        let focus = match hit {
            Some(HitTarget::Client {
                window_index,
                surface,
                surface_location,
            }) => {
                self.record_client_activity_for_window_index(
                    window_index,
                    ActivityReason::ClientInput,
                );
                Some((surface, surface_location))
            }
            _ => None,
        };
        let pointer = self.pointer.clone();
        pointer.motion(
            self,
            focus,
            &MotionEvent {
                location: self.pointer_location,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        pointer.frame(self);
    }

    pub(in crate::compositor) fn reconcile_pointer_after_output_geometry_change(&mut self) {
        // The old screen anchor is no longer meaningful after output changes.
        self.interrupt_pinch_zoom();
        let output_rects = self.output_logical_rects();
        let previous_location = self.pointer_location;
        self.pointer_location = if self.pointer_location == Point::from((0.0, 0.0)) {
            primary_output_center(&output_rects).unwrap_or(self.pointer_location)
        } else {
            clamp_point_to_outputs(self.pointer_location, &output_rects)
        };
        self.raw_pointer_location = self.pointer_location;
        if self.software_cursor_visible && self.pointer_location != previous_location {
            self.request_redraw();
        }

        if self.session_locked() {
            self.lock_pointer_motion(InputTime::now());
            return;
        }
        if self.overview_pointer_motion(InputTime::now()) {
            return;
        }
        let hit = self.hit_test(self.pointer_location);
        if !matches!(hit, Some(HitTarget::Client { .. })) {
            self.cursor_surface = None;
        }
        self.cursor_icon = match &hit {
            Some(HitTarget::ResizeBorder { edges, .. }) => resize_cursor_icon(*edges),
            _ => CursorIcon::Default,
        };
        let focus = match hit {
            Some(HitTarget::Client {
                surface,
                surface_location,
                ..
            }) => Some((surface, surface_location)),
            _ => None,
        };
        let pointer = self.pointer.clone();
        pointer.motion(
            self,
            focus,
            &MotionEvent {
                location: self.pointer_location,
                serial: SERIAL_COUNTER.next_serial(),
                time: InputTime::now(),
            },
        );
        pointer.frame(self);
        debug!(
            ?previous_location,
            location = ?self.pointer_location,
            output_rects = ?output_rects,
            "reconciled pointer after output geometry change"
        );
    }
}

fn add_synthetic_axis_to_frame(mut frame: AxisFrame, axis: Axis, amount: f64) -> AxisFrame {
    if amount == 0.0 {
        frame = frame.stop(axis);
    } else {
        frame = frame.value(axis, amount);
    }
    frame
}
