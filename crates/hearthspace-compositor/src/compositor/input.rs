#![cfg_attr(not(feature = "winit"), allow(dead_code))]

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, ButtonState, Device, Event, GestureBeginEvent,
        GestureEndEvent, GesturePinchUpdateEvent, GestureSwipeUpdateEvent, InputBackend,
        InputEvent, InputTime, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
        PointerMotionEvent, TouchEvent,
    },
    input::pointer::{AxisFrame, ButtonEvent},
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
};
use tracing::{debug, trace};

use crate::config::WHEEL_SCROLL_PIXEL_EQUIVALENT;

use super::{App, DragState, HitTarget, idle::ActivityReason};

mod keyboard;
#[cfg(feature = "winit")]
mod nested;
mod pinch;
mod pointer;
mod scroll;
mod touch;
mod touch_tracker;

pub(super) use keyboard::KeyboardShortcuts;
#[cfg(feature = "winit")]
pub(super) use nested::handle_winit_input;
use pinch::PinchInput;
pub(super) use pinch::PinchZoom;
use pointer::relative_pointer_location;
pub(super) use touch_tracker::TouchTracker;

pub(in crate::compositor) fn handle_input_event<B: InputBackend>(
    state: &mut App,
    event: InputEvent<B>,
) {
    match event {
        InputEvent::DeviceRemoved { device } => {
            state.remove_touch_device(&device.id());
            state.end_pinch_zoom(PinchInput::Touchpad, &device.id(), true, InputTime::now());
            state.end_pinch_zoom(
                PinchInput::TouchpadSwipe,
                &device.id(),
                true,
                InputTime::now(),
            );
        }
        InputEvent::GesturePinchBegin { event } => {
            state.begin_touchpad_gesture(
                PinchInput::Touchpad,
                event.device().id(),
                event.fingers(),
            );
        }
        InputEvent::GesturePinchUpdate { event } => {
            state.update_touchpad_gesture(
                PinchInput::Touchpad,
                &event.device().id(),
                event.scale(),
                event.delta(),
                event.time(),
            );
        }
        InputEvent::GesturePinchEnd { event } => {
            state.end_pinch_zoom(
                PinchInput::Touchpad,
                &event.device().id(),
                event.cancelled(),
                event.time(),
            );
        }
        InputEvent::GestureSwipeBegin { event } => {
            state.begin_touchpad_gesture(
                PinchInput::TouchpadSwipe,
                event.device().id(),
                event.fingers(),
            );
        }
        InputEvent::GestureSwipeUpdate { event } => {
            state.update_touchpad_gesture(
                PinchInput::TouchpadSwipe,
                &event.device().id(),
                1.0,
                event.delta(),
                event.time(),
            );
        }
        InputEvent::GestureSwipeEnd { event } => {
            state.end_pinch_zoom(
                PinchInput::TouchpadSwipe,
                &event.device().id(),
                event.cancelled(),
                event.time(),
            );
        }
        InputEvent::Keyboard { event } => {
            let time = event.time();
            trace!(key_code = ?event.key_code(), state = ?event.state(), ?time, "keyboard input");
            state.handle_keyboard_input(event.key_code(), event.state(), time);
        }
        InputEvent::PointerMotionAbsolute { event } => {
            let time = event.time();
            let location = event.position_transformed(state.output_logical_size());
            trace!(?location, ?time, "absolute pointer motion input");
            state.apply_pointer_motion(location, time);
        }
        InputEvent::PointerMotion { event } => {
            trace!(delta = ?event.delta(), time = ?event.time(), "relative pointer motion input");
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
                event.time(),
            );
        }
        InputEvent::PointerButton { event } => {
            if state.session_locked() {
                state.deliver_lock_button(event.button_code(), event.state(), event.time());
                return;
            }
            if event.state() == ButtonState::Pressed {
                state.interrupt_pinch_zoom();
            }
            let time = event.time();
            let is_left_button = event.button() == Some(smithay::backend::input::MouseButton::Left);
            trace!(button = event.button_code(), state = ?event.state(), ?time, "pointer button input");

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
                        if state.window_uses_output_coordinates(window_index) {
                            return;
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

            let time = event.time();
            let pointer = state.pointer.clone();
            pointer.axis(state, axis_frame_from_event(&event, time));
            pointer.frame(state);
        }
        InputEvent::TouchDown { event } => {
            let output = state.primary_output_logical_rect();
            let location =
                touch_location_in_primary_output(event.position_transformed(output.size), output);
            state.touch_down(&event.device().id(), event.slot(), location, event.time());
        }
        InputEvent::TouchMotion { event } => {
            let output = state.primary_output_logical_rect();
            let location =
                touch_location_in_primary_output(event.position_transformed(output.size), output);
            state
                .touch_input
                .motion(&event.device().id(), event.slot(), location);
        }
        InputEvent::TouchUp { event } => state.touch_input.up(&event.device().id(), event.slot()),
        InputEvent::TouchCancel { event } => {
            state.touch_input.cancel(&event.device().id(), event.slot());
            state.flush_touch_frame(&event.device().id(), event.time());
        }
        InputEvent::TouchFrame { event } => {
            state.flush_touch_frame(&event.device().id(), event.time())
        }
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
    time: InputTime,
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
