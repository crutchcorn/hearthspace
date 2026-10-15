use super::{PinchInput, PinchZoom};
use crate::{
    config::{MAX_ZOOM, MIN_ZOOM},
    geometry::{CanvasPoint, screen_to_canvas},
};

const DEVICE: &str = "touchpad-a";

#[test]
fn updates_use_absolute_scale_from_the_initial_viewport() {
    let mut pinch = PinchZoom::default();
    pinch.begin(
        PinchInput::Touchpad,
        DEVICE.into(),
        CanvasPoint { x: -37, y: 83 },
        0.75,
        (360.0, 180.0).into(),
    );

    assert_eq!(
        pinch.update(DEVICE, 1.5, (0.0, 0.0).into()),
        Some((CanvasPoint { x: 123, y: 163 }, 1.125))
    );
    assert_eq!(
        pinch.update(DEVICE, 2.0, (0.0, 0.0).into()),
        Some((CanvasPoint { x: 203, y: 203 }, 1.5))
    );
}

#[test]
fn repeated_updates_and_return_to_one_do_not_accumulate_rounding() {
    let initial_offset = CanvasPoint { x: -145, y: 283 };
    let initial_scale = 1.25;
    let anchor = (643.125, 317.875).into();
    let mut pinch = PinchZoom::default();
    pinch.begin(
        PinchInput::Touchpad,
        DEVICE.into(),
        initial_offset,
        initial_scale,
        anchor,
    );

    let first = pinch.update(DEVICE, 1.3, (0.0, 0.0).into()).unwrap();
    let anchored_canvas_point = screen_to_canvas(anchor, initial_offset, initial_scale);
    for _ in 0..20 {
        for scale in [0.65, 1.37, 1.9, 0.93, 1.3] {
            let (offset, viewport_scale) = pinch.update(DEVICE, scale, (0.0, 0.0).into()).unwrap();
            let after = screen_to_canvas(anchor, offset, viewport_scale);
            assert!((after.x - anchored_canvas_point.x).abs() <= 0.5);
            assert!((after.y - anchored_canvas_point.y).abs() <= 0.5);
        }
        assert_eq!(pinch.update(DEVICE, 1.3, (0.0, 0.0).into()), Some(first));
        assert_eq!(
            pinch.update(DEVICE, 1.0, (0.0, 0.0).into()),
            Some((initial_offset, initial_scale))
        );
    }
}

#[test]
fn zoom_limits_do_not_replace_the_gesture_baseline() {
    let mut pinch = PinchZoom::default();
    pinch.begin(
        PinchInput::Touchpad,
        DEVICE.into(),
        CanvasPoint { x: 20, y: -40 },
        1.25,
        (250.0, 100.0).into(),
    );

    // Even finite factors whose product overflows remain bounded.
    assert_eq!(
        pinch.update(DEVICE, f64::MAX, (0.0, 0.0).into()).unwrap().1,
        MAX_ZOOM
    );
    assert_eq!(pinch.update(DEVICE, 1.2, (0.0, 0.0).into()).unwrap().1, 1.5);
    assert_eq!(
        pinch.update(DEVICE, 0.1, (0.0, 0.0).into()).unwrap().1,
        MIN_ZOOM
    );
    assert_eq!(
        pinch.update(DEVICE, 0.6, (0.0, 0.0).into()).unwrap().1,
        0.75
    );
    assert_eq!(
        pinch.update(DEVICE, 1.0, (0.0, 0.0).into()),
        Some((CanvasPoint { x: 20, y: -40 }, 1.25))
    );
}

#[test]
fn invalid_scales_leave_the_gesture_active_and_usable() {
    let mut pinch = PinchZoom::default();
    pinch.begin(
        PinchInput::Touchpad,
        DEVICE.into(),
        CanvasPoint { x: 4, y: -8 },
        1.0,
        (300.0, 150.0).into(),
    );
    let previous = pinch.update(DEVICE, 1.5, (0.0, 0.0).into());

    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -2.0, -0.0, 0.0] {
        assert_eq!(pinch.update(DEVICE, invalid, (0.0, 0.0).into()), None);
        assert!(pinch.is_active());
        assert_eq!(pinch.update(DEVICE, 1.5, (0.0, 0.0).into()), previous);
    }
    assert_eq!(
        pinch.update(DEVICE, 2.0, (0.0, 0.0).into()),
        Some((CanvasPoint { x: 154, y: 67 }, 2.0))
    );
}

#[test]
fn other_devices_cannot_update_or_finish_the_active_gesture() {
    let mut pinch = PinchZoom::default();
    let initial = (CanvasPoint { x: 17, y: -29 }, 0.75);
    pinch.begin(
        PinchInput::Touchpad,
        DEVICE.into(),
        initial.0,
        initial.1,
        (123.5, 456.25).into(),
    );
    let previous = pinch.update(DEVICE, 1.5, (0.0, 0.0).into());

    assert_eq!(pinch.update("touchpad-b", 2.0, (0.0, 0.0).into()), None);
    for cancelled in [false, true] {
        assert_eq!(pinch.end("touchpad-b", cancelled), None);
        assert!(pinch.is_active());
        assert_eq!(pinch.update(DEVICE, 1.5, (0.0, 0.0).into()), previous);
    }
    assert_eq!(pinch.end(DEVICE, true), Some(initial));
    assert!(!pinch.is_active());
}

#[test]
fn normal_end_provides_no_rollback_and_rejects_late_updates() {
    let mut pinch = PinchZoom::default();
    pinch.begin(
        PinchInput::Touchpad,
        DEVICE.into(),
        CanvasPoint { x: 0, y: 0 },
        1.0,
        (300.0, 150.0).into(),
    );
    assert_eq!(pinch.update(DEVICE, 1.5, (0.0, 0.0).into()).unwrap().1, 1.5);

    assert_eq!(pinch.end(DEVICE, false), None);
    assert!(!pinch.is_active());
    assert_eq!(pinch.update(DEVICE, 2.0, (0.0, 0.0).into()), None);
    assert_eq!(pinch.end(DEVICE, true), None);
    assert_eq!(pinch.cancel(), None);
}

#[test]
fn cancel_restores_the_exact_initial_viewport_once() {
    for explicit_cancel in [false, true] {
        let mut pinch = PinchZoom::default();
        let initial = (CanvasPoint { x: -289, y: 913 }, 0.85);
        pinch.begin(
            PinchInput::Touchpad,
            DEVICE.into(),
            initial.0,
            initial.1,
            (521.125, 75.75).into(),
        );
        assert_ne!(pinch.update(DEVICE, 1.7, (0.0, 0.0).into()), Some(initial));

        let rollback = if explicit_cancel {
            pinch.cancel()
        } else {
            pinch.end(DEVICE, true)
        };
        assert_eq!(rollback, Some(initial));
        assert!(!pinch.is_active());
        assert_eq!(pinch.update(DEVICE, 1.5, (0.0, 0.0).into()), None);
        assert_eq!(pinch.cancel(), None);
    }
}

#[test]
fn a_new_interaction_prevents_late_cancellation_from_rolling_back_the_view() {
    let mut pinch = PinchZoom::default();
    pinch.begin(
        PinchInput::Touchpad,
        DEVICE.into(),
        CanvasPoint { x: 10, y: 20 },
        1.0,
        (300.0, 150.0).into(),
    );
    let visible = pinch.update(DEVICE, 1.5, (0.0, 0.0).into()).unwrap();
    // A popup grab or explicit navigation can interrupt without another pinch
    // update. Neither a delayed end nor session cancellation may undo its view.
    pinch.interrupt();
    assert_eq!(pinch.end(DEVICE, true), None);
    assert_eq!(pinch.cancel(), None);
    assert_eq!(pinch.update(DEVICE, 2.0, (0.0, 0.0).into()), None);
    pinch.begin(
        PinchInput::Touchpad,
        DEVICE.into(),
        visible.0,
        visible.1,
        (300.0, 150.0).into(),
    );
    assert_eq!(pinch.update(DEVICE, 1.0, (0.0, 0.0).into()), Some(visible));
}

#[test]
fn canvas_restore_waits_for_a_meaningful_scale_change_on_either_input() {
    for input in [PinchInput::Touchpad, PinchInput::Touchscreen] {
        for initial in [MIN_ZOOM, 1.0, MAX_ZOOM] {
            let mut pinch = PinchZoom::default();
            pinch.begin(
                input,
                DEVICE.into(),
                CanvasPoint { x: 10, y: -20 },
                initial,
                (300.0, 200.0).into(),
            );
            for factor in [1.0, 0.0, f64::NAN] {
                assert!(!pinch.needs_canvas_restore(DEVICE, factor, (0.0, 0.0).into()));
            }
            assert!(!pinch.needs_canvas_restore("another-device", 1.2, (0.0, 0.0).into()));
            if initial == MIN_ZOOM {
                assert!(!pinch.needs_canvas_restore(DEVICE, 0.5, (0.0, 0.0).into()));
            }
            if initial == MAX_ZOOM {
                assert!(!pinch.needs_canvas_restore(DEVICE, 2.0, (0.0, 0.0).into()));
            }
            let factor = if initial == MAX_ZOOM { 0.8 } else { 1.2 };
            assert!(pinch.needs_canvas_restore(DEVICE, factor, (0.0, 0.0).into()));
            pinch.prepare_canvas(CanvasPoint { x: 10, y: -20 }, Vec::new());
            assert!(!pinch.needs_canvas_restore(DEVICE, factor, (0.0, 0.0).into()));
        }
    }
}

#[test]
fn revealed_window_rebases_anchor_without_replacing_cancellation_viewport() {
    let original = CanvasPoint { x: -100, y: 200 };
    let revealed = CanvasPoint { x: 7400, y: -3100 };
    let anchor = (500.25, 300.75).into();
    for input in [PinchInput::Touchpad, PinchInput::Touchscreen] {
        let mut pinch = PinchZoom::default();
        pinch.begin(input, DEVICE.into(), original, 1.0, anchor);
        pinch.prepare_canvas(revealed, Vec::new());
        let before = screen_to_canvas(anchor, revealed, 1.0);
        let (offset, scale) = pinch.update(DEVICE, 0.75, (0.0, 0.0).into()).unwrap();
        let after = screen_to_canvas(anchor, offset, scale);
        assert!((before.x - after.x).abs() <= 0.5);
        assert!((before.y - after.y).abs() <= 0.5);
        assert_eq!(pinch.end(DEVICE, true), Some((original, 1.0)));
    }
}
