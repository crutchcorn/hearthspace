use super::*;
use crate::compositor::input::PinchZoom;

const DEVICE: &str = "touchpad";

fn begin(input: PinchInput) -> SwitcherGesture {
    SwitcherGesture::begin(input, DEVICE.into(), 4).unwrap()
}

#[test]
fn only_supported_swipe_sources_and_counts_begin() {
    for input in [
        PinchInput::Touchpad,
        PinchInput::TouchpadSwipe,
        PinchInput::Touchscreen,
    ] {
        for fingers in 0..=7 {
            assert_eq!(
                SwitcherGesture::begin(input, DEVICE.into(), fingers).is_some(),
                input != PinchInput::Touchpad && matches!(fingers, 4 | 5),
            );
        }
    }
}

#[test]
fn horizontal_swipes_step_once_then_repeat_at_detents() {
    for input in [PinchInput::TouchpadSwipe, PinchInput::Touchscreen] {
        for direction in [-1.0, 1.0] {
            let mut gesture = begin(input);
            for distance in [0.0, 10.0, 31.9] {
                assert_eq!(
                    gesture.advance(input, DEVICE, (distance * direction, 0.0).into()),
                    0
                );
            }
            assert_eq!(
                gesture.advance(input, DEVICE, (32.0 * direction, 0.0).into()),
                direction as i32
            );
            assert_eq!(
                gesture.advance(input, DEVICE, (127.9 * direction, 0.0).into()),
                0
            );
            assert_eq!(
                gesture.advance(input, DEVICE, (224.0 * direction, 0.0).into()),
                2 * direction as i32
            );
            assert_eq!(
                gesture.advance(input, DEVICE, (224.0 * direction, 0.0).into()),
                0
            );
        }
    }
}

#[test]
fn reverse_motion_has_hysteresis_and_can_switch_back_repeatedly() {
    let input = PinchInput::Touchscreen;
    let mut gesture = begin(input);
    assert_eq!(gesture.advance(input, DEVICE, (-32.0, 0.0).into()), -1);
    for distance in [-33.0, -31.0, 0.0, 15.9] {
        assert_eq!(gesture.advance(input, DEVICE, (distance, 0.0).into()), 0);
    }
    assert_eq!(gesture.advance(input, DEVICE, (16.0, 0.0).into()), 1);
    assert_eq!(gesture.advance(input, DEVICE, (112.0, 0.0).into()), 1);
    assert_eq!(gesture.advance(input, DEVICE, (64.0, 0.0).into()), -1);
    assert_eq!(gesture.advance(input, DEVICE, (-32.0, 0.0).into()), -1);
}

#[test]
fn vertical_swipes_and_ambiguous_diagonals_do_not_cycle() {
    let input = PinchInput::TouchpadSwipe;
    let mut gesture = begin(input);
    assert_eq!(gesture.advance(input, DEVICE, (33.0, 33.0).into()), 0);
    assert_eq!(gesture.advance(input, DEVICE, (33.0, 60.0).into()), 0);
    assert_eq!(gesture.advance(input, DEVICE, (300.0, 60.0).into()), 0);
    assert!(
        gesture.vertical,
        "vertical intent cannot later become a window switch"
    );
}

#[test]
fn wrong_sources_devices_and_invalid_updates_preserve_accumulated_motion() {
    let input = PinchInput::TouchpadSwipe;
    let mut gesture = begin(input);
    assert_eq!(gesture.advance(input, DEVICE, (-20.0, 0.0).into()), 0);
    assert_eq!(gesture.advance(input, "other", (-400.0, 0.0).into()), 0);
    assert_eq!(
        gesture.advance(PinchInput::Touchscreen, DEVICE, (-400.0, 0.0).into()),
        0
    );
    for translation in [(f64::NAN, 0.0), (0.0, f64::INFINITY), (f64::MAX, 0.0)] {
        assert_eq!(gesture.advance(input, DEVICE, translation.into()), 0);
    }
    assert_eq!(gesture.translation, Point::from((-20.0, 0.0)));
    assert_eq!(gesture.advance(input, DEVICE, (-32.0, 0.0).into()), -1);
}

#[test]
fn large_updates_have_bounded_work() {
    let input = PinchInput::TouchpadSwipe;
    let mut gesture = begin(input);
    assert_eq!(
        gesture.advance(input, DEVICE, (1_000_000.0, 0.0).into()),
        32
    );
}

#[test]
fn shared_reset_discards_switcher_candidate_without_canvas_rollback() {
    for interrupt in [false, true] {
        let mut pinch = PinchZoom {
            switcher: Some(begin(PinchInput::TouchpadSwipe)),
            ..Default::default()
        };
        assert!(pinch.is_active());
        assert!(
            !pinch.interrupt_switcher_pinch(),
            "switcher pinch cleanup preserves switcher ownership"
        );
        if interrupt {
            pinch.interrupt();
        } else {
            assert_eq!(pinch.cancel(), None);
        }
        assert!(!pinch.is_active());
        assert!(pinch.switcher.is_none());
    }
}
