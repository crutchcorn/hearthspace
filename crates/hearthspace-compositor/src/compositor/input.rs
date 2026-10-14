#![cfg_attr(not(feature = "winit"), allow(dead_code))]

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, ButtonState, Event, InputBackend, InputEvent,
        KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent, TouchEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent},
    },
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
};
use tracing::{debug, trace};

use crate::config::WHEEL_SCROLL_PIXEL_EQUIVALENT;

use super::{App, DragState, HitTarget, idle::ActivityReason};

mod pointer;
mod scroll;

use pointer::relative_pointer_location;

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
            // libinput reports accelerated relative deltas already in the
            // compositor's logical coordinate space, so they are applied
            // directly to the logical pointer location (as Smithay's reference
            // compositor does). They must NOT be divided by the output scale:
            // the pointer position is logical, and monitor scale only governs
            // how that logical position is *presented* (cursor plane, surface
            // buffers), not how far a physical motion moves the pointer. Scaling
            // the delta made the HiDPI pointer track at 1/scale speed, so the
            // rendered cursor drifted away from where input was actually being
            // routed.
            state.apply_pointer_motion(
                relative_pointer_location(state.raw_pointer_location, event.delta()),
                event.time_msec(),
            );
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
                        if let Some(surface) = state.windows[window_index].surface.wl_surface() {
                            state.set_keyboard_focus_to_window(window_index, surface);
                        }
                        state.drag = Some(DragState {
                            window_id: state.windows[window_index].id,
                            pointer_start: state.raw_pointer_location,
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
                        if let Some(surface) = state.windows[window_index].surface.wl_surface() {
                            state.set_keyboard_focus_to_window(window_index, surface);
                        }
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
                        if let Some(surface) = state.windows[window_index].surface.wl_surface() {
                            state.set_keyboard_focus_to_window(window_index, surface);
                        }
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
            if state.zoom_from_scroll_amount(scroll_amount_for_axis(&event, Axis::Vertical)) {
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

            let primary_output = state.primary_output_logical_rect();
            let location = touch_location_in_primary_output(
                event.position_transformed(primary_output.size),
                primary_output,
            );
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

            let primary_output = state.primary_output_logical_rect();
            let location = touch_location_in_primary_output(
                event.position_transformed(primary_output.size),
                primary_output,
            );
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

fn touch_location_in_primary_output(
    local_location: Point<f64, Logical>,
    primary_output: Rectangle<i32, Logical>,
) -> Point<f64, Logical> {
    local_location + primary_output.loc.to_f64()
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

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Point, Rectangle};

    use super::touch_location_in_primary_output;

    #[test]
    fn touch_location_is_relative_to_primary_output_not_combined_desktop() {
        let primary = Rectangle::<i32, Logical>::new((2560, 400).into(), (2560, 1600).into());

        let location = touch_location_in_primary_output(Point::from((1280.0, 800.0)), primary);

        assert_eq!(location, Point::from((3840.0, 1200.0)));
    }
}
