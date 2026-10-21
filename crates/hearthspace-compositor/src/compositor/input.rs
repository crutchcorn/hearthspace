#![cfg_attr(not(feature = "winit"), allow(dead_code))]

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, Device, DeviceCapability, Event, GestureBeginEvent,
        GestureEndEvent, GesturePinchUpdateEvent, GestureSwipeUpdateEvent, InputBackend,
        InputEvent, InputTime, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
        PointerMotionEvent, TouchEvent,
    },
    input::pointer::AxisFrame,
    utils::{Logical, Point, Rectangle},
};
use tracing::trace;

use crate::config::WHEEL_SCROLL_PIXEL_EQUIVALENT;

use super::{App, HitTarget, idle::ActivityReason};

mod buttons;
mod favorite_drawing;
mod interactive;
mod keyboard;
#[cfg(feature = "winit")]
mod nested;
mod overview;
mod pinch;
mod pointer;
mod recovery;
mod remote;
mod scroll;
mod shell_grab;
mod titlebar;
mod touch;
mod touch_tracker;
mod ui_touch;

pub(super) use favorite_drawing::FavoriteDrawing;
pub(super) use keyboard::KeyboardShortcuts;
#[cfg(feature = "winit")]
pub(super) use nested::handle_winit_input;
pub(super) use overview::OverviewInput;
use pinch::PinchInput;
pub(super) use pinch::PinchZoom;
use pointer::relative_pointer_location;
pub(super) use remote::RemoteInput;
pub(super) use titlebar::TitlebarInput;
pub(super) use touch_tracker::TouchTracker;
pub(super) use ui_touch::UiTouch;

pub(in crate::compositor) fn handle_input_event<B: InputBackend>(
    state: &mut App,
    event: InputEvent<B>,
) {
    match event {
        InputEvent::DeviceRemoved { device } => {
            state.favorite_device_removed(&device.id());
            if device.has_capability(DeviceCapability::Pointer) {
                state.release_local_pointer();
                state.favorite_pointer_removed();
            }
            if device.has_capability(DeviceCapability::Keyboard) {
                // Key state is shared across physical and agent keyboards. A
                // removed keyboard cannot finish a held Meta/Alt chord. Cancel
                // bare-key actions before releasing all held input state.
                state.reset_session_input();
            }
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
            state.shortcuts.cancel_meta_tap();
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
            state.shortcuts.cancel_meta_tap();
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
            state.handle_pointer_button(event.button_code(), event.state(), event.time());
        }
        InputEvent::PointerAxis { event } => {
            trace!(
                horizontal = ?scroll_amount_for_axis(&event, Axis::Horizontal),
                vertical = ?scroll_amount_for_axis(&event, Axis::Vertical),
                "pointer axis input"
            );
            if state.layout_group_axis(
                scroll_amount_for_axis(&event, Axis::Horizontal).unwrap_or(0.),
                scroll_amount_for_axis(&event, Axis::Vertical).unwrap_or(0.),
            ) || state.zoom_from_scroll_amount(scroll_amount_for_axis(&event, Axis::Vertical))
            {
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
            if !state.favorite_touch_motion(&event.device().id(), event.slot(), location) {
                state.motion_ui_touch(&event.device().id(), event.slot(), location, event.time());
                state
                    .touch_input
                    .motion(&event.device().id(), event.slot(), location);
            }
        }
        InputEvent::TouchUp { event } => {
            if !state.favorite_touch_up(&event.device().id(), event.slot(), false) {
                state.end_ui_touch(&event.device().id(), event.slot(), false, event.time());
                state.touch_input.up(&event.device().id(), event.slot());
            }
        }
        InputEvent::TouchCancel { event } => {
            if state.touch_pointer_pressed
                && state.touch_input.has_device_input(&event.device().id())
            {
                state.cancel_titlebar_pointer();
                state.cancel_canvas_search_touch();
                if state.window_switcher_active() {
                    state.cancel_window_switcher_touch();
                }
            }
            state.favorite_touch_up(&event.device().id(), event.slot(), true);
            state.end_ui_touch(&event.device().id(), event.slot(), true, event.time());
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
