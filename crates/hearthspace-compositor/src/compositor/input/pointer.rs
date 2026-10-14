use smithay::{
    backend::input::{Axis, ButtonState, InputTime, KeyState},
    input::pointer::{AxisFrame, ButtonEvent, MotionEvent},
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
};
use tracing::{debug, trace};

use crate::compositor::{
    App, DragState, HitTarget, cursor::CursorIcon, idle::ActivityReason,
    windows::resize_cursor_icon,
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
        trace!(button, ?state, "synthesizing pointer button");
        if state == ButtonState::Pressed {
            self.interrupt_pinch_zoom();
        }
        let time = InputTime::now();
        let is_left_button = button == 0x110;

        if is_left_button
            && state == ButtonState::Released
            && (self.drag.is_some() || self.resize.is_some())
        {
            debug!("ending synthetic pointer drag/resize from button release");
            self.drag = None;
            self.finish_resize();
            let pointer = self.pointer.clone();
            pointer.button(
                self,
                &ButtonEvent {
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                    button,
                    state,
                },
            );
            pointer.frame(self);
            return;
        }

        if is_left_button && state == ButtonState::Pressed {
            match self.hit_test(self.pointer_location) {
                Some(HitTarget::CloseButton { window_index }) => {
                    debug!(
                        window_id = self.windows[window_index].id,
                        "close button clicked from synthetic input"
                    );
                    self.windows[window_index].surface.send_close();
                    self.drag = None;
                    return;
                }
                Some(HitTarget::TitleBar { window_index }) => {
                    let window_index = self.raise_window(window_index);
                    if let Some(surface) = self.windows[window_index].surface.wl_surface() {
                        self.set_keyboard_focus_to_window(window_index, surface);
                    }
                    if self.window_uses_output_coordinates(window_index) {
                        return;
                    }
                    self.drag = Some(DragState {
                        window_id: self.windows[window_index].id,
                        pointer_start: self.raw_pointer_location,
                        window_start: self.windows[window_index].position,
                    });
                    debug!(
                        window_id = self.windows[window_index].id,
                        "started titlebar drag from synthetic input"
                    );
                    self.request_redraw();
                    return;
                }
                Some(HitTarget::ResizeBorder {
                    window_index,
                    edges,
                }) => {
                    let window_index = self.raise_window(window_index);
                    if let Some(surface) = self.windows[window_index].surface.wl_surface() {
                        self.set_keyboard_focus_to_window(window_index, surface);
                    }
                    debug!(
                        window_id = self.windows[window_index].id,
                        ?edges,
                        "started resize from synthetic pointer press"
                    );
                    self.start_resize(window_index, edges);
                    return;
                }
                Some(HitTarget::Client { window_index, .. }) => {
                    let window_index = self.raise_window(window_index);
                    if let Some(surface) = self.windows[window_index].surface.wl_surface() {
                        self.set_keyboard_focus_to_window(window_index, surface);
                    }
                }
                None => {
                    debug!("clearing keyboard focus from synthetic pointer press on canvas");
                    self.clear_keyboard_focus();
                }
            }
        } else if is_left_button
            && state == ButtonState::Released
            && matches!(
                self.hit_test(self.pointer_location),
                Some(HitTarget::CloseButton { .. })
            )
        {
            return;
        }

        let focus = match self.hit_test(self.pointer_location) {
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

        match focus.clone() {
            Some((surface, _)) => {
                if state == ButtonState::Pressed
                    && let Some(window_index) = self.window_index_for_surface(&surface)
                {
                    self.set_keyboard_focus_to_window(window_index, surface);
                }
            }
            _ if is_left_button && state == ButtonState::Pressed => return,
            _ => {}
        }

        let pointer = self.pointer.clone();
        pointer.button(
            self,
            &ButtonEvent {
                serial: SERIAL_COUNTER.next_serial(),
                time,
                button,
                state,
            },
        );
        pointer.frame(self);
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

    pub(super) fn apply_pointer_motion(&mut self, location: Point<f64, Logical>, time: InputTime) {
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

        if let Some(drag) = self.drag.as_ref() {
            let delta = self.raw_pointer_location - drag.pointer_start;
            let new_position = crate::geometry::CanvasPoint {
                x: drag.window_start.x + (delta.x / self.viewport_scale).round() as i32,
                y: drag.window_start.y + (delta.y / self.viewport_scale).round() as i32,
            };
            let window_id = drag.window_id;
            // The compositor owns the cursor while dragging window chrome.
            self.cursor_surface = None;
            if let Some(window) = self.window_mut_by_id(window_id) {
                window.position = new_position;
                trace!(window_id, ?new_position, "updated window drag position");
                self.request_redraw();
            }
            return;
        }

        if let Some(resize) = self.resize.as_ref() {
            let edges = resize.edges;
            self.cursor_icon = resize_cursor_icon(edges);
            // The compositor owns the cursor while resizing.
            self.cursor_surface = None;
            self.update_resize(self.raw_pointer_location);
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

fn primary_output_center(output_rects: &[Rectangle<i32, Logical>]) -> Option<Point<f64, Logical>> {
    let rect = output_rects.first()?;
    Some(Point::from((
        f64::from(rect.loc.x) + f64::from(rect.size.w.max(1)) / 2.0,
        f64::from(rect.loc.y) + f64::from(rect.size.h.max(1)) / 2.0,
    )))
}

fn clamp_point_to_outputs(
    location: Point<f64, Logical>,
    output_rects: &[Rectangle<i32, Logical>],
) -> Point<f64, Logical> {
    output_rects
        .iter()
        .map(|rect| clamp_point_to_rect(location, *rect))
        .min_by(|a, b| {
            squared_distance(location, *a)
                .partial_cmp(&squared_distance(location, *b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(location)
}

struct PointerConstraint {
    visible: Point<f64, Logical>,
    raw: Point<f64, Logical>,
}

fn constrain_pointer_to_outputs(
    previous: Point<f64, Logical>,
    location: Point<f64, Logical>,
    output_rects: &[Rectangle<i32, Logical>],
) -> PointerConstraint {
    let previous_rect = rect_containing_point(previous, output_rects)
        .or_else(|| nearest_rect_to_point(previous, output_rects));

    if rect_containing_point(location, output_rects).is_some() {
        return PointerConstraint {
            visible: location,
            raw: location,
        };
    }

    let visible = previous_rect
        .map(|rect| clamp_point_to_rect(location, rect))
        .unwrap_or_else(|| clamp_point_to_outputs(location, output_rects));
    PointerConstraint {
        visible,
        raw: visible,
    }
}

fn rect_containing_point(
    point: Point<f64, Logical>,
    rects: &[Rectangle<i32, Logical>],
) -> Option<Rectangle<i32, Logical>> {
    rects
        .iter()
        .copied()
        .find(|rect| point_in_rect(point, *rect))
}

fn rect_index_containing_point(
    point: Point<f64, Logical>,
    rects: &[Rectangle<i32, Logical>],
) -> Option<usize> {
    rects.iter().position(|rect| point_in_rect(point, *rect))
}

fn nearest_rect_to_point(
    point: Point<f64, Logical>,
    rects: &[Rectangle<i32, Logical>],
) -> Option<Rectangle<i32, Logical>> {
    rects.iter().copied().min_by(|a, b| {
        squared_distance(point, clamp_point_to_rect(point, *a))
            .partial_cmp(&squared_distance(point, clamp_point_to_rect(point, *b)))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn clamp_point_to_rect(
    location: Point<f64, Logical>,
    rect: Rectangle<i32, Logical>,
) -> Point<f64, Logical> {
    const EDGE_EPSILON: f64 = 0.000_001;
    let max_x = f64::from(rect.loc.x + rect.size.w.max(1)) - EDGE_EPSILON;
    let max_y = f64::from(rect.loc.y + rect.size.h.max(1)) - EDGE_EPSILON;
    Point::from((
        location.x.clamp(f64::from(rect.loc.x), max_x),
        location.y.clamp(f64::from(rect.loc.y), max_y),
    ))
}

fn squared_distance(a: Point<f64, Logical>, b: Point<f64, Logical>) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

fn point_in_rect(point: Point<f64, Logical>, rect: Rectangle<i32, Logical>) -> bool {
    point.x >= f64::from(rect.loc.x)
        && point.y >= f64::from(rect.loc.y)
        && point.x < f64::from(rect.loc.x + rect.size.w.max(1))
        && point.y < f64::from(rect.loc.y + rect.size.h.max(1))
}

pub(super) fn relative_pointer_location(
    raw_location: Point<f64, Logical>,
    delta: Point<f64, Logical>,
) -> Point<f64, Logical> {
    raw_location + delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_motion_starts_from_visible_location_after_pointer_hits_edge() {
        let output_rects = [Rectangle::from_size((100, 100).into())];
        let raw_location = Point::from((120.0, 50.0));
        let visible_location = clamp_point_to_outputs(raw_location, &output_rects);

        assert!((visible_location.x - 99.999_999).abs() < f64::EPSILON);
        assert_eq!(visible_location.y, 50.0);
        let relative = relative_pointer_location(visible_location, Point::from((20.0, 0.0)));
        assert!((relative.x - 119.999_999).abs() < f64::EPSILON);
        assert_eq!(relative.y, 50.0);
    }

    #[test]
    fn pointer_crosses_between_touching_outputs() {
        let output_rects = [
            Rectangle::new((0, 0).into(), (100, 100).into()),
            Rectangle::new((100, 0).into(), (100, 100).into()),
        ];

        let constrained = constrain_pointer_to_outputs(
            Point::from((99.999_999, 50.0)),
            Point::from((100.000_001, 50.0)),
            &output_rects,
        );

        assert_eq!(constrained.visible, Point::from((100.000_001, 50.0)));
        assert_eq!(constrained.raw, constrained.visible);
    }

    #[test]
    fn raw_pointer_does_not_accumulate_in_real_output_gaps() {
        let output_rects = [
            Rectangle::new((0, 0).into(), (100, 100).into()),
            Rectangle::new((120, 0).into(), (100, 100).into()),
        ];

        let constrained = constrain_pointer_to_outputs(
            Point::from((99.999_999, 50.0)),
            Point::from((110.0, 50.0)),
            &output_rects,
        );

        assert_eq!(constrained.visible, Point::from((99.999_999, 50.0)));
        assert_eq!(constrained.raw, constrained.visible);
    }

    #[test]
    fn pointer_accepts_locations_inside_any_output() {
        let output_rects = [
            Rectangle::new((0, 0).into(), (100, 100).into()),
            Rectangle::new((120, 0).into(), (100, 100).into()),
        ];

        let constrained = constrain_pointer_to_outputs(
            Point::from((99.999_999, 50.0)),
            Point::from((130.0, 50.0)),
            &output_rects,
        );

        assert_eq!(constrained.visible, Point::from((130.0, 50.0)));
        assert_eq!(constrained.raw, constrained.visible);
    }

    #[test]
    fn pointer_clamps_to_real_outputs_not_dead_space_in_bounding_box() {
        let output_rects = [
            Rectangle::new((0, 0).into(), (2560, 1440).into()),
            Rectangle::new((2560, 0).into(), (1366, 768).into()),
        ];

        let clamped = clamp_point_to_outputs(Point::from((3200.0, 1200.0)), &output_rects);
        assert_eq!(clamped.x, 3200.0);
        assert!((clamped.y - 767.999_999).abs() < f64::EPSILON);
    }

    #[test]
    fn primary_output_center_uses_first_active_output() {
        let output_rects = [
            Rectangle::new((100, 20).into(), (800, 600).into()),
            Rectangle::new((900, 20).into(), (640, 480).into()),
        ];

        assert_eq!(
            primary_output_center(&output_rects),
            Some(Point::from((500.0, 320.0)))
        );
    }
}
