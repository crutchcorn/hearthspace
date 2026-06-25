use smithay::{
    backend::{
        input::{
            AbsolutePositionEvent, Axis, ButtonState, Event, InputEvent, KeyboardKeyEvent,
            PointerAxisEvent, PointerButtonEvent,
        },
        winit::WinitInput,
    },
    input::{
        keyboard::{FilterResult, keysyms},
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    utils::Serial,
};

use crate::config::{SCROLL_ZOOM_SENSITIVITY, WHEEL_SCROLL_PIXEL_EQUIVALENT};

use super::{App, DragState, HitTarget, idle::ActivityReason};

pub(super) fn handle_input_event(state: &mut App, event: InputEvent<WinitInput>) {
    match event {
        InputEvent::Keyboard { event } => {
            let time = event.time_msec();
            state.record_focused_client_activity(ActivityReason::ClientInput);
            let keyboard = state.keyboard.clone();
            keyboard.input::<(), _>(
                state,
                event.key_code(),
                event.state(),
                Serial::from(0),
                time,
                |_, _, _| FilterResult::Forward,
            );
        }
        InputEvent::PointerMotionAbsolute { event } => {
            let time = event.time_msec();
            let location = event.position_transformed(state.output_size.to_logical(1));
            state.pointer_location = location;

            if let Some(drag) = state.drag.as_ref() {
                let delta = location - drag.pointer_start;
                let new_position = super::CanvasPoint {
                    x: drag.window_start.x + (delta.x / state.viewport_scale).round() as i32,
                    y: drag.window_start.y + (delta.y / state.viewport_scale).round() as i32,
                };
                let window_id = drag.window_id;
                if let Some(window) = state.window_mut_by_id(window_id) {
                    window.position = new_position;
                    state.request_redraw();
                }
                return;
            }

            let focus = match state.hit_test(location) {
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
            let pointer = state.pointer.clone();
            pointer.motion(
                state,
                focus,
                &MotionEvent {
                    location,
                    serial: Serial::from(0),
                    time,
                },
            );
            pointer.frame(state);
        }
        InputEvent::PointerButton { event } => {
            let time = event.time_msec();
            let is_left_button = event.button() == Some(smithay::backend::input::MouseButton::Left);

            if is_left_button && event.state() == ButtonState::Released && state.drag.is_some() {
                state.drag = None;
                // Forward the release to the pointer so the implicit grab
                // established by the originating press (e.g. a client-initiated
                // `move_request`) is released. Swallowing it here would leave the
                // grab stuck on the dragged window, misrouting all later pointer
                // input to it.
                let pointer = state.pointer.clone();
                pointer.button(
                    state,
                    &ButtonEvent {
                        serial: Serial::from(0),
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
                        state.request_redraw();
                        return;
                    }
                    Some(HitTarget::Client { window_index, .. }) => {
                        let window_index = state.raise_window(window_index);
                        let surface = state.windows[window_index].surface.wl_surface().clone();
                        state.set_keyboard_focus_to_window(window_index, surface);
                    }
                    None => {
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
                    serial: Serial::from(0),
                    time,
                    button: event.button_code(),
                    state: event.state(),
                },
            );
            pointer.frame(state);
        }
        InputEvent::PointerAxis { event } => {
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
        _ => {}
    }
}

fn axis_frame_from_event(event: &impl PointerAxisEvent<WinitInput>, time: u32) -> AxisFrame {
    let mut frame = AxisFrame::new(time)
        .source(event.source())
        .relative_direction(Axis::Horizontal, event.relative_direction(Axis::Horizontal))
        .relative_direction(Axis::Vertical, event.relative_direction(Axis::Vertical));

    frame = add_axis_to_frame(frame, event, Axis::Horizontal);
    add_axis_to_frame(frame, event, Axis::Vertical)
}

fn add_axis_to_frame(
    mut frame: AxisFrame,
    event: &impl PointerAxisEvent<WinitInput>,
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

fn scroll_amount_for_axis(event: &impl PointerAxisEvent<WinitInput>, axis: Axis) -> Option<f64> {
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

    fn zoom_from_scroll(&mut self, event: &impl PointerAxisEvent<WinitInput>) -> bool {
        let Some(scroll_amount) = scroll_amount_for_axis(event, Axis::Vertical) else {
            return false;
        };
        if scroll_amount == 0.0 {
            return true;
        }

        self.advance_viewport_animation();
        self.animate_zoom_around_viewport_center((-scroll_amount * SCROLL_ZOOM_SENSITIVITY).exp());
        true
    }
}
