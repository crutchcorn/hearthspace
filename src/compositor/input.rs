#![cfg_attr(not(feature = "winit"), allow(dead_code))]

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, ButtonState, Event, InputBackend, InputEvent, KeyState,
        KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent, TouchEvent,
    },
    input::{
        keyboard::{FilterResult, keysyms},
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    utils::{Logical, Point, SERIAL_COUNTER},
};
use tracing::{debug, trace};

use crate::config::{SCROLL_ZOOM_SENSITIVITY, WHEEL_SCROLL_PIXEL_EQUIVALENT};

use super::{
    App, DragState, HitTarget, cursor::CursorIcon, idle::ActivityReason,
    windows::resize_cursor_icon,
};

pub(in crate::compositor) fn handle_input_event<B: InputBackend>(
    state: &mut App,
    event: InputEvent<B>,
) {
    match event {
        InputEvent::Keyboard { event } => {
            let time = event.time_msec();
            trace!(key_code = ?event.key_code(), state = ?event.state(), time, "keyboard input");
            state.record_focused_client_activity(ActivityReason::ClientInput);
            let keyboard = state.keyboard.clone();
            keyboard.input::<(), _>(
                state,
                event.key_code(),
                event.state(),
                SERIAL_COUNTER.next_serial(),
                time,
                |_, _, _| FilterResult::Forward,
            );
        }
        InputEvent::PointerMotionAbsolute { event } => {
            let time = event.time_msec();
            let location = event.position_transformed(state.output_logical_size());
            trace!(?location, time, "absolute pointer motion input");
            state.apply_pointer_motion(location, time);
        }
        InputEvent::PointerMotion { event } => {
            trace!(delta = ?event.delta(), time = event.time_msec(), "relative pointer motion input");
            state.apply_pointer_motion(state.pointer_location + event.delta(), event.time_msec());
        }
        InputEvent::PointerButton { event } => {
            let time = event.time_msec();
            let is_left_button = event.button() == Some(smithay::backend::input::MouseButton::Left);
            trace!(button = event.button_code(), state = ?event.state(), time, "pointer button input");

            if is_left_button
                && event.state() == ButtonState::Released
                && (state.drag.is_some() || state.resize.is_some())
            {
                debug!("ending pointer drag/resize from button release");
                state.drag = None;
                state.finish_resize();
                // Forward the release to the pointer so the implicit grab
                // established by the originating press (e.g. a client-initiated
                // `move_request`) is released. Swallowing it here would leave the
                // grab stuck on the dragged window, misrouting all later pointer
                // input to it.
                let pointer = state.pointer.clone();
                pointer.button(
                    state,
                    &ButtonEvent {
                        serial: SERIAL_COUNTER.next_serial(),
                        time,
                        button: event.button_code(),
                        state: event.state(),
                    },
                );
                pointer.frame(state);
                return;
            }

            if is_left_button && event.state() == ButtonState::Pressed {
                match state.hit_test(state.pointer_location) {
                    Some(HitTarget::CloseButton { window_index }) => {
                        debug!(
                            window_id = state.windows[window_index].id,
                            "close button clicked"
                        );
                        state.windows[window_index].surface.send_close();
                        state.drag = None;
                        return;
                    }
                    Some(HitTarget::TitleBar { window_index }) => {
                        let window_index = state.raise_window(window_index);
                        let surface = state.windows[window_index].surface.wl_surface().clone();
                        state.set_keyboard_focus_to_window(window_index, surface);
                        state.drag = Some(DragState {
                            window_id: state.windows[window_index].id,
                            pointer_start: state.pointer_location,
                            window_start: state.windows[window_index].position,
                        });
                        debug!(
                            window_id = state.windows[window_index].id,
                            "started titlebar drag"
                        );
                        state.request_redraw();
                        return;
                    }
                    Some(HitTarget::ResizeBorder {
                        window_index,
                        edges,
                    }) => {
                        let window_index = state.raise_window(window_index);
                        let surface = state.windows[window_index].surface.wl_surface().clone();
                        state.set_keyboard_focus_to_window(window_index, surface);
                        debug!(
                            window_id = state.windows[window_index].id,
                            ?edges,
                            "started resize from pointer press"
                        );
                        state.start_resize(window_index, edges);
                        return;
                    }
                    Some(HitTarget::Client { window_index, .. }) => {
                        let window_index = state.raise_window(window_index);
                        let surface = state.windows[window_index].surface.wl_surface().clone();
                        state.set_keyboard_focus_to_window(window_index, surface);
                    }
                    None => {
                        debug!("clearing keyboard focus from pointer press on canvas");
                        state.clear_keyboard_focus();
                    }
                }
            } else if is_left_button
                && event.state() == ButtonState::Released
                && matches!(
                    state.hit_test(state.pointer_location),
                    Some(HitTarget::CloseButton { .. })
                )
            {
                return;
            }

            let focus = match state.hit_test(state.pointer_location) {
                Some(HitTarget::Client {
                    window_index,
                    surface,
                    surface_location,
                }) => {
                    state.record_client_activity_for_window_index(
                        window_index,
                        ActivityReason::ClientInput,
                    );
                    Some((surface, surface_location))
                }
                _ => None,
            };

            match focus.clone() {
                Some((surface, _)) => {
                    if event.state() == ButtonState::Pressed
                        && let Some(window_index) = state.window_index_for_surface(&surface)
                    {
                        state.set_keyboard_focus_to_window(window_index, surface);
                    }
                }
                _ if is_left_button && event.state() == ButtonState::Pressed => {
                    return;
                }
                _ => {}
            }

            let pointer = state.pointer.clone();
            pointer.button(
                state,
                &ButtonEvent {
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                    button: event.button_code(),
                    state: event.state(),
                },
            );
            pointer.frame(state);
        }
        InputEvent::PointerAxis { event } => {
            trace!(
                horizontal = ?scroll_amount_for_axis(&event, Axis::Horizontal),
                vertical = ?scroll_amount_for_axis(&event, Axis::Vertical),
                "pointer axis input"
            );
            if state.scroll_zoom_active() && state.zoom_from_scroll(&event) {
                return;
            }

            if let Some(HitTarget::Client { window_index, .. }) =
                state.hit_test(state.pointer_location)
            {
                state.record_client_activity_for_window_index(
                    window_index,
                    ActivityReason::ClientInput,
                );
            }

            let time = event.time_msec();
            let pointer = state.pointer.clone();
            pointer.axis(state, axis_frame_from_event(&event, time));
            pointer.frame(state);
        }
        InputEvent::TouchDown { event } => {
            let slot = event.slot();
            let time = event.time_msec();
            if state.active_touch_slot.is_some() {
                trace!(?slot, time, "ignoring additional touch contact");
                return;
            }

            let location = event.position_transformed(state.output_logical_size());
            trace!(?slot, ?location, time, "single-touch down input");
            state.active_touch_slot = Some(slot);
            state.apply_pointer_motion(location, time);
            state.synthesize_pointer_button(0x110, ButtonState::Pressed);
        }
        InputEvent::TouchMotion { event } => {
            let slot = event.slot();
            let time = event.time_msec();
            if state.active_touch_slot != Some(slot) {
                trace!(?slot, time, "ignoring inactive touch motion");
                return;
            }

            let location = event.position_transformed(state.output_logical_size());
            trace!(?slot, ?location, time, "single-touch motion input");
            state.apply_pointer_motion(location, time);
        }
        InputEvent::TouchUp { event } => {
            let slot = event.slot();
            let time = event.time_msec();
            if state.active_touch_slot != Some(slot) {
                trace!(?slot, time, "ignoring inactive touch up");
                return;
            }

            trace!(?slot, time, "single-touch up input");
            state.synthesize_pointer_button(0x110, ButtonState::Released);
            state.active_touch_slot = None;
        }
        InputEvent::TouchCancel { event } => {
            let slot = event.slot();
            let time = event.time_msec();
            if state.active_touch_slot != Some(slot) {
                trace!(?slot, time, "ignoring inactive touch cancel");
                return;
            }

            debug!(?slot, time, "single-touch input cancelled");
            state.synthesize_pointer_button(0x110, ButtonState::Released);
            state.active_touch_slot = None;
        }
        InputEvent::TouchFrame { .. } => trace!("single-touch frame input"),
        _ => {}
    }
}

fn axis_frame_from_event<B: InputBackend>(
    event: &impl PointerAxisEvent<B>,
    time: u32,
) -> AxisFrame {
    let mut frame = AxisFrame::new(time)
        .source(event.source())
        .relative_direction(Axis::Horizontal, event.relative_direction(Axis::Horizontal))
        .relative_direction(Axis::Vertical, event.relative_direction(Axis::Vertical));

    frame = add_axis_to_frame(frame, event, Axis::Horizontal);
    add_axis_to_frame(frame, event, Axis::Vertical)
}

fn add_axis_to_frame<B: InputBackend>(
    mut frame: AxisFrame,
    event: &impl PointerAxisEvent<B>,
    axis: Axis,
) -> AxisFrame {
    if let Some(amount) = scroll_amount_for_axis(event, axis) {
        if amount == 0.0 {
            frame = frame.stop(axis);
        } else {
            frame = frame.value(axis, amount);
        }
    }

    if let Some(v120) = event.amount_v120(axis) {
        frame = frame.v120(axis, v120.round() as i32);
    }

    frame
}

fn scroll_amount_for_axis<B: InputBackend>(
    event: &impl PointerAxisEvent<B>,
    axis: Axis,
) -> Option<f64> {
    event.amount(axis).or_else(|| {
        event
            .amount_v120(axis)
            .map(|amount| amount / 120.0 * WHEEL_SCROLL_PIXEL_EQUIVALENT)
    })
}

fn is_super_keysym(keysym: u32) -> bool {
    matches!(
        keysym,
        keysyms::KEY_Super_L
            | keysyms::KEY_Super_R
            | keysyms::KEY_Meta_L
            | keysyms::KEY_Meta_R
            | keysyms::KEY_Hyper_L
            | keysyms::KEY_Hyper_R
    )
}

impl App {
    pub(super) fn synthesize_key(&mut self, evdev_keycode: u32, state: KeyState) {
        trace!(evdev_keycode, ?state, "synthesizing key input");
        self.record_focused_client_activity(ActivityReason::ClientInput);
        let keyboard = self.keyboard.clone();
        keyboard.input::<(), _>(
            self,
            evdev_keycode.saturating_add(8).into(),
            state,
            SERIAL_COUNTER.next_serial(),
            event_time_msec(),
            |_, _, _| FilterResult::Forward,
        );
    }

    pub(super) fn synthesize_pointer_motion_abs(&mut self, location: Point<f64, Logical>) {
        trace!(?location, "synthesizing absolute pointer motion");
        self.apply_pointer_motion(location, event_time_msec());
    }

    pub(super) fn synthesize_pointer_motion_rel(&mut self, delta: Point<f64, Logical>) {
        trace!(?delta, "synthesizing relative pointer motion");
        self.apply_pointer_motion(self.pointer_location + delta, event_time_msec());
    }

    pub(super) fn synthesize_pointer_button(&mut self, button: u32, state: ButtonState) {
        trace!(button, ?state, "synthesizing pointer button");
        let time = event_time_msec();
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
                    let surface = self.windows[window_index].surface.wl_surface().clone();
                    self.set_keyboard_focus_to_window(window_index, surface);
                    self.drag = Some(DragState {
                        window_id: self.windows[window_index].id,
                        pointer_start: self.pointer_location,
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
                    let surface = self.windows[window_index].surface.wl_surface().clone();
                    self.set_keyboard_focus_to_window(window_index, surface);
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
                    let surface = self.windows[window_index].surface.wl_surface().clone();
                    self.set_keyboard_focus_to_window(window_index, surface);
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

    pub(super) fn synthesize_axis(&mut self, horizontal: f64, vertical: f64) {
        trace!(horizontal, vertical, "synthesizing pointer axis");
        if let Some(HitTarget::Client { window_index, .. }) = self.hit_test(self.pointer_location) {
            self.record_client_activity_for_window_index(window_index, ActivityReason::ClientInput);
        }

        let time = event_time_msec();
        let mut frame = AxisFrame::new(time);
        frame = add_synthetic_axis_to_frame(frame, Axis::Horizontal, horizontal);
        frame = add_synthetic_axis_to_frame(frame, Axis::Vertical, vertical);

        let pointer = self.pointer.clone();
        pointer.axis(self, frame);
        pointer.frame(self);
    }

    fn apply_pointer_motion(&mut self, location: Point<f64, Logical>, time: u32) {
        let previous_location = self.pointer_location;
        self.pointer_location = clamp_point_to_output(location, self.output_logical_size());
        if self.software_cursor_visible && self.pointer_location != previous_location {
            self.request_redraw();
        }

        if let Some(drag) = self.drag.as_ref() {
            let delta = self.pointer_location - drag.pointer_start;
            let new_position = super::CanvasPoint {
                x: drag.window_start.x + (delta.x / self.viewport_scale).round() as i32,
                y: drag.window_start.y + (delta.y / self.viewport_scale).round() as i32,
            };
            let window_id = drag.window_id;
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
            self.update_resize(self.pointer_location);
            return;
        }

        let hit = self.hit_test(self.pointer_location);
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

    fn super_modifier_active(&self) -> bool {
        self.keyboard.modifier_state().logo
            || self.keyboard.with_pressed_keysyms(|pressed| {
                pressed
                    .iter()
                    .any(|key| is_super_keysym(key.modified_sym().raw()))
            })
    }

    fn scroll_zoom_active(&self) -> bool {
        self.scroll_zooms_without_super || self.super_modifier_active()
    }

    fn zoom_from_scroll<B: InputBackend>(&mut self, event: &impl PointerAxisEvent<B>) -> bool {
        let Some(scroll_amount) = scroll_amount_for_axis(event, Axis::Vertical) else {
            return false;
        };
        if scroll_amount == 0.0 {
            return true;
        }

        self.advance_viewport_animation();
        debug!(scroll_amount, "zooming viewport from scroll input");
        self.animate_zoom_around_viewport_center((-scroll_amount * SCROLL_ZOOM_SENSITIVITY).exp());
        true
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

fn clamp_point_to_output(
    location: Point<f64, Logical>,
    output_size: smithay::utils::Size<i32, Logical>,
) -> Point<f64, Logical> {
    Point::from((
        location
            .x
            .clamp(0.0, f64::from(output_size.w.saturating_sub(1))),
        location
            .y
            .clamp(0.0, f64::from(output_size.h.saturating_sub(1))),
    ))
}

fn event_time_msec() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u32)
        .unwrap_or_default()
}
