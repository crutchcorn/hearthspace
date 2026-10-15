use super::*;
use crate::compositor::input::PinchZoom;

#[test]
fn only_four_and_five_finger_pinches_begin_overview() {
    for input in [
        PinchInput::Touchpad,
        PinchInput::Touchscreen,
        PinchInput::TouchpadSwipe,
    ] {
        for fingers in 0..=8 {
            assert_eq!(
                OverviewGesture::begin(input, "device".into(), fingers).is_some(),
                input != PinchInput::TouchpadSwipe && matches!(fingers, 4 | 5),
            );
        }
    }
}

#[test]
fn inward_opens_outward_closes_and_subthreshold_does_nothing() {
    for input in [PinchInput::Touchpad, PinchInput::Touchscreen] {
        for fingers in [4, 5] {
            for (scale, expected) in [
                (0.5, Some(OverviewAction::Open)),
                (0.8, Some(OverviewAction::Open)),
                (0.8001, None),
                (1.0, None),
                (1.2499, None),
                (1.25, Some(OverviewAction::Close)),
                (2.0, Some(OverviewAction::Close)),
            ] {
                let mut gesture = OverviewGesture::begin(input, "device".into(), fingers).unwrap();
                gesture.update(input, "device", scale);
                assert_eq!(gesture.action(false), expected);
            }
        }
    }
}

#[test]
fn cancellation_never_commits_even_after_crossing_either_threshold() {
    for input in [PinchInput::Touchpad, PinchInput::Touchscreen] {
        for fingers in [4, 5] {
            for scale in [0.3, 0.8, 1.25, 3.0] {
                let mut gesture = OverviewGesture::begin(input, "device".into(), fingers).unwrap();
                gesture.update(input, "device", scale);
                assert_eq!(gesture.action(true), None);
            }
        }
    }
}

#[test]
fn final_scale_controls_direction_instead_of_latching_an_earlier_crossing() {
    let mut gesture = OverviewGesture::begin(PinchInput::Touchpad, "device".into(), 4).unwrap();
    for scale in [0.6, 1.8, 0.7, 1.0] {
        gesture.update(PinchInput::Touchpad, "device", scale);
    }
    assert_eq!(gesture.action(false), None);
}

#[test]
fn other_devices_inputs_and_invalid_scales_cannot_change_the_candidate() {
    let mut gesture = OverviewGesture::begin(PinchInput::Touchpad, "device".into(), 5).unwrap();
    gesture.update(PinchInput::Touchscreen, "device", 0.5);
    gesture.update(PinchInput::TouchpadSwipe, "device", 0.5);
    gesture.update(PinchInput::Touchpad, "other", 0.5);
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        gesture.update(PinchInput::Touchpad, "device", scale);
    }
    assert_eq!(gesture.action(false), None);
}

#[test]
fn shared_cancellation_and_interruption_discard_overview_without_viewport_rollback() {
    for interrupt in [false, true] {
        let mut pinch = PinchZoom {
            overview: OverviewGesture::begin(PinchInput::Touchpad, "device".into(), 4),
            ..Default::default()
        };
        pinch
            .overview
            .as_mut()
            .unwrap()
            .update(PinchInput::Touchpad, "device", 0.5);
        assert!(pinch.is_active());
        if interrupt {
            pinch.interrupt();
        } else {
            assert_eq!(pinch.cancel(), None);
        }
        assert!(!pinch.is_active());
        assert!(pinch.overview.is_none());
    }
}

#[test]
fn overview_only_interruption_preserves_three_finger_cancellation_snapshot() {
    let mut pinch = PinchZoom::default();
    let initial = (crate::geometry::CanvasPoint { x: -45, y: 97 }, 0.75);
    pinch.begin(
        PinchInput::Touchscreen,
        "screen".into(),
        initial.0,
        initial.1,
        (320.0, 180.0).into(),
    );
    let visible = pinch.update("screen", 0.7, (100.0, -20.0).into());
    assert_ne!(visible, Some(initial));
    assert!(!pinch.interrupt_overview());
    assert!(pinch.is_active());
    assert_eq!(pinch.update("screen", 0.7, (100.0, -20.0).into()), visible);
    assert_eq!(pinch.cancel(), Some(initial));
}

#[test]
fn overview_only_interruption_discards_the_candidate_once() {
    for input in [PinchInput::Touchpad, PinchInput::Touchscreen] {
        let mut pinch = PinchZoom {
            overview: OverviewGesture::begin(input, "device".into(), 5),
            ..Default::default()
        };
        pinch
            .overview
            .as_mut()
            .unwrap()
            .update(input, "device", 0.5);
        assert!(pinch.interrupt_overview());
        assert!(!pinch.is_active());
        assert!(!pinch.interrupt_overview());
        assert_eq!(pinch.cancel(), None);
    }
}
