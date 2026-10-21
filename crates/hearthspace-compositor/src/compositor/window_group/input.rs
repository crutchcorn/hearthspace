use smithay::{
    backend::input::{ButtonState, InputTime},
    input::pointer::MotionEvent,
    utils::{Logical, Point, SERIAL_COUNTER},
};

use super::{
    App, Gesture,
    model::{Lasso, drag_delta, moved_position},
};
use crate::{
    compositor::{HitTarget, ManagedWindowKind, cursor::CursorIcon},
    geometry::rect_contains,
};

impl App {
    fn group_hits_shell(&self) -> bool {
        matches!(self.hit_test(self.pointer_location), Some(HitTarget::Client { window_index, .. })
            if self.windows[window_index].kind != ManagedWindowKind::Normal)
    }

    pub(in crate::compositor) fn group_pointer_button(
        &mut self,
        button: u32,
        state: ButtonState,
    ) -> bool {
        if state == ButtonState::Pressed {
            self.dismiss_group_outside_press(self.pointer_location, button == 0x110);
        }
        if button != 0x110 {
            return false;
        }
        if !self.dispatching_touch && self.group_touch_owns_input() {
            self.cancel_ui_touch();
        }
        if state == ButtonState::Released && self.window_group.pointer_held {
            self.finish_group_pointer_sequence(!self.group_pointer_visible());
            return true;
        }
        if state != ButtonState::Pressed || !self.group_active() {
            return false;
        }
        if !self.group_pointer_visible() {
            self.clear_window_group();
            return false;
        }
        if self.pointer.is_grabbed() {
            return false;
        }
        // Real hit order matters: a covered member cannot steal a press from
        // an unrelated window, shell panel, or popup in front of the group.
        let shell = self.group_hits_shell();
        let anchor = self
            .group_window_under_pointer()
            .filter(|id| self.window_group.selected.contains(id))
            .and_then(|id| self.windows.iter().position(|window| window.id == id))
            .map(|index| self.window_canvas_rect(index).loc);
        let gather = !shell
            && !self.window_group.selecting
            && !self.window_group.selected.is_empty()
            && rect_contains(self.group_action_rect(), self.pointer_location);
        if shell || (!self.window_group.selecting && anchor.is_none() && !gather) {
            self.clear_window_group();
            return false;
        }
        self.interrupt_pinch_zoom();
        self.viewport_animation = None;
        self.window_group.pointer_held = true;
        self.follow_group_pointer_output();
        if self.window_group.selecting {
            self.window_group.replace_selection(Default::default());
            self.window_group.gesture = Some(Gesture::Select {
                lasso: Lasso::new(self.pointer_location),
            });
        } else if gather {
            if self.window_group.pending.is_none()
                && let Err(error) = self.start_group_gather()
            {
                self.window_group.error = Some(error);
            }
        } else {
            self.window_group.flights.clear();
            for window in &mut self.windows {
                if self.window_group.selected.contains(&window.id) {
                    window.pending_resize = None;
                }
            }
            self.window_group.gesture = Some(Gesture::Drag {
                origin: self.raw_pointer_location,
                scale: self.viewport_scale,
                anchor: anchor.expect("group drag starts on a selected window"),
                positions: self
                    .windows
                    .iter()
                    .filter(|window| self.window_group.selected.contains(&window.id))
                    .map(|window| (window.id, window.position))
                    .collect(),
            });
            let ids: Vec<_> = self.window_group.selected.iter().copied().collect();
            for id in ids {
                if let Some(index) = self.windows.iter().position(|window| window.id == id) {
                    self.raise_window(index);
                }
            }
        }
        self.request_redraw();
        true
    }

    /// Touch cancellation must abort before its tracker emits a normal release.
    pub(in crate::compositor) fn finish_group_pointer_sequence(&mut self, cancelled: bool) {
        self.window_group.pointer_held = false;
        let was_lasso = matches!(
            self.window_group.gesture.take(),
            Some(Gesture::Select { .. })
        );
        if self.window_group.selecting {
            self.window_group.selecting = false;
            if cancelled || !was_lasso {
                self.window_group.replace_selection(Default::default());
            }
        }
        self.request_redraw();
    }

    pub(in crate::compositor) fn group_has_pointer_capture(&self) -> bool {
        self.window_group.pointer_held
    }

    pub(in crate::compositor) fn group_touch_blocked(&self) -> bool {
        self.group_has_pointer_capture()
            || self.drag.is_some()
            || self.resize.is_some()
            || self.pointer.is_grabbed()
            || self.keyboard.is_grabbed()
    }

    /// Run before specialized shell/favorite delivery, including native touch.
    pub(in crate::compositor) fn dismiss_group_outside_press(
        &mut self,
        location: Point<f64, Logical>,
        primary: bool,
    ) {
        if !self.group_active() {
            return;
        }
        let visible = self
            .output_logical_rects()
            .into_iter()
            .find(|output| output.contains(location.to_i32_round()))
            .is_some_and(|output| self.group_visible_on_output(output));
        let shell = matches!(self.hit_test(location), Some(HitTarget::Client { window_index, .. })
            if self.windows[window_index].kind != ManagedWindowKind::Normal);
        if visible && self.window_group.selecting && primary && !shell {
            return;
        }
        let selected = self
            .group_window_at(location)
            .is_some_and(|id| self.window_group.selected.contains(&id));
        let gather = !self.window_group.selecting
            && !self.window_group.selected.is_empty()
            && rect_contains(self.group_action_rect(), location);
        if !visible || shell || (!selected && !gather) {
            self.clear_window_group();
        }
    }

    fn group_window_under_pointer(&self) -> Option<u64> {
        self.group_window_at(self.pointer_location)
    }

    fn group_window_at(&self, location: Point<f64, Logical>) -> Option<u64> {
        let index = match self.hit_test(location)? {
            HitTarget::Client { window_index, .. }
            | HitTarget::TitlebarButton { window_index, .. }
            | HitTarget::TitleBar { window_index }
            | HitTarget::ResizeBorder { window_index, .. } => window_index,
        };
        self.group_window_eligible(index)
            .then_some(self.windows[index].id)
    }

    pub(in crate::compositor) fn group_pointer_motion(&mut self, time: InputTime) -> bool {
        if !self.dispatching_touch && self.group_touch_owns_input() {
            self.cancel_ui_touch();
        }
        if !self.group_pointer_visible() {
            if self.window_group.pointer_held {
                self.finish_group_pointer_sequence(true);
                // The owner still owes a release after the overlay disappears.
                self.window_group.pointer_held = true;
                self.cursor_icon = CursorIcon::Default;
                return self.group_captured_pointer_motion(time);
            }
            return false;
        }
        if self.pointer.is_grabbed() && !self.window_group.pointer_held {
            return false;
        }
        self.request_redraw();
        if let Some(gesture) = self.window_group.gesture.take() {
            self.window_group.gesture = Some(match gesture {
                Gesture::Select { mut lasso } => {
                    lasso.push(self.pointer_location);
                    let selected = self
                        .windows
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| {
                            self.group_window_eligible(*index)
                                && lasso.selects(self.window_screen_bounds(*index))
                        })
                        .map(|(_, window)| window.id)
                        .collect();
                    self.window_group.replace_selection(selected);
                    Gesture::Select { lasso }
                }
                Gesture::Drag {
                    origin,
                    scale,
                    anchor,
                    positions,
                } => {
                    self.follow_group_pointer_output();
                    let delta = drag_delta(
                        anchor,
                        &positions,
                        self.raw_pointer_location - origin,
                        scale,
                        self.keyboard.modifier_state().alt,
                    );
                    for &(id, start) in &positions {
                        if let Some(index) = self.windows.iter().position(|window| window.id == id)
                            && self.group_window_eligible(index)
                        {
                            self.set_group_window_position(
                                index,
                                moved_position(start, delta, 1.0),
                            );
                        }
                    }
                    Gesture::Drag {
                        origin,
                        scale,
                        anchor,
                        positions,
                    }
                }
            });
        } else if self.group_hits_shell()
            || !(self.window_group.selecting
                || rect_contains(self.group_action_rect(), self.pointer_location)
                || self
                    .group_window_under_pointer()
                    .is_some_and(|id| self.window_group.selected.contains(&id)))
        {
            return false;
        }
        self.cursor_surface = None;
        self.cursor_icon = if matches!(self.window_group.gesture, Some(Gesture::Drag { .. })) {
            CursorIcon::Grabbing
        } else if self.window_group.selecting {
            CursorIcon::Crosshair
        } else if rect_contains(self.group_action_rect(), self.pointer_location) {
            CursorIcon::Pointer
        } else {
            CursorIcon::Grab
        };
        self.group_captured_pointer_motion(time)
    }

    fn group_captured_pointer_motion(&mut self, time: InputTime) -> bool {
        self.cursor_surface = None;
        let pointer = self.pointer.clone();
        pointer.motion(
            self,
            None,
            &MotionEvent {
                location: self.pointer_location,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        pointer.frame(self);
        true
    }

    pub(in crate::compositor) fn group_keyboard_cycle(&mut self, reverse: bool) {
        let mut ids: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(index, _)| self.group_window_eligible(*index))
            .map(|(_, window)| window.id)
            .collect();
        ids.sort_unstable();
        self.window_group.keyboard_target = crate::compositor::navigation::cycle_target(
            &ids,
            self.window_group.keyboard_target,
            reverse,
        );
        if let Some(id) = self.window_group.keyboard_target {
            let _ = self.focus_window(id);
        }
        self.request_redraw();
    }

    fn follow_group_pointer_output(&mut self) {
        if let Some(output) = self.capture_outputs().into_iter().find(|output| {
            smithay::utils::Rectangle::new(
                (output.logical_x, output.logical_y).into(),
                (output.logical_width, output.logical_height).into(),
            )
            .contains(self.pointer_location.to_i32_round())
        }) {
            self.window_group.output = Some(output.name);
        }
    }

    pub(in crate::compositor) fn group_keyboard_toggle_target(&mut self) {
        let Some(id) = self.window_group.keyboard_target else {
            return;
        };
        if !self
            .windows
            .iter()
            .position(|window| window.id == id)
            .is_some_and(|index| self.group_window_eligible(index))
        {
            return;
        }
        let mut selected = self.window_group.selected.clone();
        if !selected.remove(&id) {
            selected.insert(id);
        }
        self.window_group.replace_selection(selected);
        self.request_redraw();
    }
}
