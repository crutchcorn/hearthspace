//! Adapt winit's unframed touch input and raw physical coordinates.
use smithay::{
    backend::{
        input::{AbsolutePositionEvent, Device, Event, InputEvent, InputTime, TouchEvent},
        winit::WinitInput,
    },
    utils::{Logical, Physical, Point, Rectangle, Size},
};

use super::{super::App, handle_input_event};

pub(in crate::compositor) fn handle_winit_input(state: &mut App, event: InputEvent<WinitInput>) {
    // Pinned winit/Smithay normalizes touch down/up Y by width but motion Y by
    // height. Raw physical positions are consistent across all three events.
    match event {
        InputEvent::TouchDown { event } => {
            let location = touch_location(
                event.position(),
                state.output_size(),
                state.primary_output_logical_rect(),
            );
            state.touch_down(&event.device().id(), event.slot(), location, event.time());
        }
        InputEvent::TouchMotion { event } => {
            let location = touch_location(
                event.position(),
                state.output_size(),
                state.primary_output_logical_rect(),
            );
            if !state.favorite_touch_motion(&event.device().id(), event.slot(), location) {
                state.motion_ui_touch(&event.device().id(), event.slot(), location, event.time());
                state
                    .touch_input
                    .motion(&event.device().id(), event.slot(), location);
            }
        }
        other => handle_input_event(state, other),
    }
    // WinitInput::TouchFrameEvent is UnusedEvent; every delivered event is a
    // complete sample. Native libinput continues to use its actual frames.
    let actions = state.touch_input.frame_all();
    state.apply_touch_actions(actions, InputTime::now());
}

fn touch_location(
    physical: Point<f64, smithay::utils::Raw>,
    size: Size<i32, Physical>,
    output: Rectangle<i32, Logical>,
) -> Point<f64, Logical> {
    (
        f64::from(output.loc.x) + physical.x * f64::from(output.size.w) / f64::from(size.w.max(1)),
        f64::from(output.loc.y) + physical.y * f64::from(output.size.h) / f64::from(size.h.max(1)),
    )
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_coordinates_use_both_dimensions_and_output_scale() {
        let output = Rectangle::new((40, 20).into(), (1280, 800).into());
        assert_eq!(
            touch_location((1280.0, 800.0).into(), (2560, 1600).into(), output),
            (680.0, 420.0).into(),
        );
    }
}
