use smithay::utils::{Logical, Point};

use super::{PinchInput, PinchZoom};
use crate::{
    config::{MAX_ZOOM, MIN_ZOOM},
    geometry::{CanvasPoint, canvas_to_screen, screen_to_canvas},
};

const DEVICE: &str = "gesture-device";
const OFFSET: CanvasPoint = CanvasPoint { x: -125, y: 350 };

fn begin(input: PinchInput, scale: f64) -> PinchZoom {
    let mut gesture = PinchZoom::default();
    gesture.begin(input, DEVICE.into(), OFFSET, scale, (400.25, 250.75).into());
    gesture
}

#[test]
fn all_pan_directions_move_content_with_fingers_at_nonunit_scale() {
    for input in [
        PinchInput::Touchpad,
        PinchInput::TouchpadSwipe,
        PinchInput::Touchscreen,
    ] {
        let gesture = begin(input, 0.5);
        for (translation, expected) in [
            ((30.0, 0.0), CanvasPoint { x: -185, y: 350 }),
            ((-30.0, 0.0), CanvasPoint { x: -65, y: 350 }),
            ((0.0, 20.0), CanvasPoint { x: -125, y: 310 }),
            ((0.0, -20.0), CanvasPoint { x: -125, y: 390 }),
            ((30.0, -20.0), CanvasPoint { x: -185, y: 390 }),
        ] {
            assert_eq!(
                gesture.update(DEVICE, 1.0, translation.into()),
                Some((expected, 0.5))
            );
        }
    }
}

#[test]
fn simultaneous_pan_and_zoom_keep_the_original_canvas_anchor_under_the_fingers() {
    let anchor: Point<f64, Logical> = (400.25, 250.75).into();
    let translation: Point<f64, Logical> = (43.125, -27.875).into();
    for input in [PinchInput::Touchpad, PinchInput::Touchscreen] {
        let gesture = begin(input, 1.25);
        let canvas_anchor = screen_to_canvas(anchor, OFFSET, 1.25);
        for factor in [0.1, 0.7, 1.0, 1.3, 100.0] {
            let (offset, scale) = gesture.update(DEVICE, factor, translation).unwrap();
            let after = canvas_to_screen(canvas_anchor, offset, scale);
            let expected = anchor + translation;
            assert!((after.x - expected.x).abs() <= 0.5 * scale + 1e-9);
            assert!((after.y - expected.y).abs() <= 0.5 * scale + 1e-9);
            assert_eq!(scale, (1.25 * factor).clamp(MIN_ZOOM, MAX_ZOOM));
        }
    }
}

#[test]
fn touchscreen_translations_are_absolute_and_reversible() {
    let gesture = begin(PinchInput::Touchscreen, 1.25);
    let translated = gesture.update(DEVICE, 1.0, (50.0, -25.0).into());
    for _ in 0..20 {
        assert_eq!(
            gesture.update(DEVICE, 1.0, (50.0, -25.0).into()),
            translated
        );
        assert_eq!(
            gesture.update(DEVICE, 1.0, (0.0, 0.0).into()),
            Some((OFFSET, 1.25))
        );
    }
}

#[test]
fn touchpad_fractional_deltas_accumulate_without_per_event_rounding() {
    for input in [PinchInput::Touchpad, PinchInput::TouchpadSwipe] {
        let mut gesture = begin(input, 1.25);
        let mut translation = (0.0, 0.0).into();
        for _ in 0..40 {
            translation = gesture
                .accumulate_translation(input, DEVICE, 1.0, (0.1, -0.15).into())
                .unwrap();
            // Rendering an intermediate frame must not become the next baseline.
            gesture.update(DEVICE, 1.0, translation).unwrap();
        }
        assert!((translation.x - 4.0).abs() < 1e-9);
        assert!((translation.y + 6.0).abs() < 1e-9);
        assert_eq!(
            gesture.update(DEVICE, 1.0, translation),
            Some((CanvasPoint { x: -128, y: 355 }, 1.25))
        );
        let single_update = begin(input, 1.25).update(DEVICE, 1.0, (4.0, -6.0).into());
        assert_eq!(gesture.update(DEVICE, 1.0, translation), single_update);
        for _ in 0..40 {
            translation = gesture
                .accumulate_translation(input, DEVICE, 1.0, (-0.1, 0.15).into())
                .unwrap();
        }
        assert_eq!(
            gesture.update(DEVICE, 1.0, translation),
            Some((OFFSET, 1.25))
        );
    }
}

#[test]
fn pan_still_works_at_zoom_limits_without_replacing_the_baseline() {
    for input in [
        PinchInput::Touchpad,
        PinchInput::TouchpadSwipe,
        PinchInput::Touchscreen,
    ] {
        for (scale, factor) in [(MIN_ZOOM, 0.1), (MAX_ZOOM, 100.0)] {
            let gesture = begin(input, scale);
            for translation in [(0.0, 0.0), (0.1, -0.1)] {
                assert_eq!(
                    gesture.update(DEVICE, factor, translation.into()),
                    Some((OFFSET, scale))
                );
            }
            let translation = (20.0, -10.0).into();
            let updated = gesture.update(DEVICE, factor, translation).unwrap();
            assert_eq!(updated.1, scale);
            assert_ne!(updated.0, OFFSET);
        }
    }
}

#[test]
fn cancelled_pan_and_combined_gestures_restore_the_original_view() {
    let anchor: Point<f64, Logical> = (400.25, 250.75).into();
    let translation: Point<f64, Logical> = (80.25, 42.75).into();
    for input in [
        PinchInput::Touchpad,
        PinchInput::TouchpadSwipe,
        PinchInput::Touchscreen,
    ] {
        for factor in [1.0, 0.75] {
            let mut gesture = begin(input, 1.0);
            let canvas_anchor = screen_to_canvas(anchor, OFFSET, 1.0);
            let (offset, scale) = gesture.update(DEVICE, factor, translation).unwrap();
            let after = canvas_to_screen(canvas_anchor, offset, scale);
            assert!((after.x - (anchor.x + translation.x)).abs() <= 0.5 * scale);
            assert!((after.y - (anchor.y + translation.y)).abs() <= 0.5 * scale);
            assert_eq!(gesture.end(DEVICE, true), Some((OFFSET, 1.0)));
            assert_eq!(gesture.cancel(), None);
        }
    }
}

#[test]
fn interrupted_or_completed_pan_cannot_be_rolled_back_by_a_late_cancel() {
    for input in [PinchInput::TouchpadSwipe, PinchInput::Touchscreen] {
        for interrupt in [false, true] {
            let mut gesture = begin(input, 1.0);
            assert_ne!(
                gesture.update(DEVICE, 1.0, (80.0, 40.0).into()),
                Some((OFFSET, 1.0))
            );
            if interrupt {
                gesture.interrupt();
            } else {
                assert_eq!(gesture.end(DEVICE, false), None);
            }
            assert_eq!(gesture.end(DEVICE, true), None);
            assert_eq!(gesture.cancel(), None);
            assert!(!gesture.is_active());
        }
    }
}

#[test]
fn relative_updates_are_isolated_by_kind_and_device() {
    for input in [PinchInput::Touchpad, PinchInput::TouchpadSwipe] {
        let mut gesture = begin(input, 1.0);
        for other in [
            PinchInput::Touchpad,
            PinchInput::TouchpadSwipe,
            PinchInput::Touchscreen,
        ] {
            if other != input {
                assert!(!gesture.accepts(other));
                assert_eq!(
                    gesture.accumulate_translation(other, DEVICE, 1.0, (40.0, 50.0).into()),
                    None
                );
            }
        }
        assert_eq!(
            gesture.accumulate_translation(input, "another-device", 1.0, (40.0, 50.0).into()),
            None
        );
        assert_eq!(gesture.end("another-device", true), None);
        assert_eq!(
            gesture.accumulate_translation(input, DEVICE, 1.0, (1.5, 2.5).into()),
            Some((1.5, 2.5).into())
        );
    }
}

#[test]
fn malformed_relative_updates_do_not_poison_later_motion() {
    let input = PinchInput::Touchpad;
    let mut gesture = begin(input, MIN_ZOOM);
    gesture
        .accumulate_translation(input, DEVICE, 1.0, (1.0, -2.0).into())
        .unwrap();
    for scale in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -1.0] {
        assert_eq!(
            gesture.accumulate_translation(input, DEVICE, scale, (10.0, 20.0).into()),
            None
        );
    }
    for delta in [
        (f64::NAN, 0.0),
        (0.0, f64::INFINITY),
        (f64::NEG_INFINITY, 0.0),
        (f64::MAX, f64::MAX),
    ] {
        assert_eq!(
            gesture.accumulate_translation(input, DEVICE, 1.0, delta.into()),
            None
        );
    }
    assert_eq!(
        gesture.accumulate_translation(input, DEVICE, 1.0, (3.0, 4.0).into()),
        Some((4.0, 2.0).into())
    );
    assert_eq!(
        gesture.update(DEVICE, 1.0, (4.0, 2.0).into()),
        Some((CanvasPoint { x: -133, y: 346 }, MIN_ZOOM))
    );
}

#[test]
fn overflow_does_not_replace_the_last_finite_touchpad_accumulator() {
    let input = PinchInput::TouchpadSwipe;
    let mut gesture = begin(input, MAX_ZOOM);
    assert_eq!(
        gesture.accumulate_translation(input, DEVICE, 1.0, (f64::MAX, 0.0).into()),
        Some((f64::MAX, 0.0).into())
    );
    assert_eq!(
        gesture.accumulate_translation(input, DEVICE, 1.0, (f64::MAX, 0.0).into()),
        None
    );
    let translation = gesture
        .accumulate_translation(input, DEVICE, 1.0, (-f64::MAX, 0.0).into())
        .unwrap();
    assert_eq!(translation, (0.0, 0.0).into());
    assert_eq!(
        gesture.update(DEVICE, 1.0, translation),
        Some((OFFSET, MAX_ZOOM))
    );
}

#[test]
fn malformed_absolute_translations_and_integer_limits_do_not_replace_baseline() {
    let gesture = begin(PinchInput::Touchscreen, MIN_ZOOM);
    for translation in [(f64::NAN, 0.0), (0.0, f64::INFINITY), (f64::MAX, f64::MAX)] {
        assert_eq!(gesture.update(DEVICE, 1.0, translation.into()), None);
    }
    assert_eq!(
        gesture.update(DEVICE, 1.0, (1e30, -1e30).into()),
        Some((
            CanvasPoint {
                x: i32::MIN,
                y: i32::MAX
            },
            MIN_ZOOM
        ))
    );
    assert_eq!(
        gesture.update(DEVICE, 1.0, (0.0, 0.0).into()),
        Some((OFFSET, MIN_ZOOM))
    );
}
