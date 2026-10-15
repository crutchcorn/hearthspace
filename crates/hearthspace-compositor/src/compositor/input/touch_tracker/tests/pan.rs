use super::*;

pub(super) fn transform(tracker: &mut TouchTracker, scale: f64, translation: Position) {
    let anchor = point(30.0, 20.0);
    for (id, position) in [
        (0, point(0.0, 0.0)),
        (1, point(60.0, 0.0)),
        (2, point(30.0, 60.0)),
    ] {
        tracker.motion(
            DEVICE,
            slot(id),
            anchor + (position - anchor).upscale(scale) + translation,
        );
    }
}

fn assert_transform(actions: &[TouchAction], expected_scale: f64, expected: Position) {
    let [TouchAction::UpdatePinch { scale, translation }] = actions else {
        panic!("unexpected actions: {actions:?}");
    };
    assert!((scale - expected_scale).abs() < 1e-12, "{actions:?}");
    assert!((translation.x - expected.x).abs() < 1e-12, "{actions:?}");
    assert!((translation.y - expected.y).abs() < 1e-12, "{actions:?}");
}

#[test]
fn parallel_three_finger_motion_reports_every_direction_without_scaling() {
    for translation in [
        point(-90.0, 0.0),
        point(90.0, 0.0),
        point(0.0, -60.0),
        point(0.0, 60.0),
        point(-90.0, -60.0),
        point(-90.0, 60.0),
        point(90.0, -60.0),
        point(90.0, 60.0),
    ] {
        let mut tracker = TouchTracker::default();
        begin(&mut tracker);
        transform(&mut tracker, 1.0, translation);
        assert_transform(&frame(&mut tracker), 1.0, translation);
        assert!(frame(&mut tracker).is_empty());
    }
}

#[test]
fn pan_and_zoom_are_absolute_and_return_to_the_same_baseline() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    for _ in 0..3 {
        for (scale, translation) in [
            (2.0, point(18.0, -12.0)),
            (0.5, point(-24.0, 36.0)),
            (1.0, point(0.0, 0.0)),
        ] {
            transform(&mut tracker, scale, translation);
            assert_transform(&frame(&mut tracker), scale, translation);
        }
    }
}

#[test]
fn rotation_preserves_scale_and_reports_only_centroid_displacement() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    for (id, position) in [
        (0, point(150.0, 190.0)),
        (1, point(150.0, 250.0)),
        (2, point(90.0, 220.0)),
    ] {
        tracker.motion(DEVICE, slot(id), position);
    }
    assert_transform(&frame(&mut tracker), 1.0, point(100.0, 200.0));
}

#[test]
fn final_pan_and_zoom_in_a_lift_frame_precede_normal_end() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    transform(&mut tracker, 2.0, point(90.0, -60.0));
    tracker.up(DEVICE, slot(0));
    let mut actions = frame(&mut tracker);
    assert_eq!(
        actions.pop(),
        Some(TouchAction::EndPinch { cancelled: false })
    );
    assert_transform(&actions, 2.0, point(90.0, -60.0));
    tracker.motion(DEVICE, slot(1), point(10.0, 20.0));
    tracker.up(DEVICE, slot(1));
    tracker.up(DEVICE, slot(2));
    assert!(frame(&mut tracker).is_empty());
    begin(&mut tracker);
    transform(&mut tracker, 1.0, point(-30.0, 30.0));
    assert_transform(&frame(&mut tracker), 1.0, point(-30.0, 30.0));
}

#[test]
fn fourth_contact_and_cancel_discard_uncommitted_transform_and_suppress_contacts() {
    for fourth_contact in [false, true] {
        let mut tracker = TouchTracker::default();
        begin(&mut tracker);
        transform(&mut tracker, 0.5, point(-60.0, 30.0));
        assert_transform(&frame(&mut tracker), 0.5, point(-60.0, 30.0));
        transform(&mut tracker, 2.0, point(90.0, -60.0));
        if fourth_contact {
            tracker.down(DEVICE, slot(3), point(200.0, 100.0));
            tracker.up(DEVICE, slot(3));
        } else {
            tracker.cancel(DEVICE, slot(0));
        }
        assert_eq!(
            frame(&mut tracker),
            vec![TouchAction::EndPinch { cancelled: true }]
        );
        tracker.motion(DEVICE, slot(1), point(10.0, 20.0));
        for id in 0..3 {
            tracker.up(DEVICE, slot(id));
        }
        assert!(frame(&mut tracker).is_empty());
        begin(&mut tracker);
    }
}

#[test]
fn invalid_geometry_does_not_replace_the_pan_or_zoom_baseline() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for position in [point(invalid, 0.0), point(0.0, invalid)] {
            tracker.motion(DEVICE, slot(0), position);
            assert!(frame(&mut tracker).is_empty());
            transform(&mut tracker, 2.0, point(-30.0, 60.0));
            assert_transform(&frame(&mut tracker), 2.0, point(-30.0, 60.0));
        }
    }
    // A finite coordinate can still overflow radius calculations. Recovery
    // must use the original centroid/radius rather than the previous update.
    tracker.motion(DEVICE, slot(0), point(f64::MAX, -f64::MAX));
    assert!(frame(&mut tracker).is_empty());
    transform(&mut tracker, 0.5, point(90.0, -30.0));
    assert_transform(&frame(&mut tracker), 0.5, point(90.0, -30.0));
}

#[test]
fn invalid_initial_geometry_waits_for_a_finite_centroid_and_radius() {
    let mut tracker = TouchTracker::default();
    tracker.down(DEVICE, slot(0), point(f64::NAN, 200.0));
    tracker.down(DEVICE, slot(1), point(360.0, 200.0));
    tracker.down(DEVICE, slot(2), point(330.0, 260.0));
    assert!(frame(&mut tracker).is_empty());
    tracker.motion(DEVICE, slot(0), point(300.0, 200.0));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::BeginPinch {
            anchor: point(330.0, 220.0)
        }]
    );
    for (id, position) in [
        (0, point(180.0, 240.0)),
        (1, point(300.0, 240.0)),
        (2, point(240.0, 360.0)),
    ] {
        tracker.motion(DEVICE, slot(id), position);
    }
    assert_transform(&frame(&mut tracker), 2.0, point(-90.0, 60.0));
}

#[test]
fn device_removal_after_combined_motion_cancels_once_and_resets_the_baseline() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    transform(&mut tracker, 2.0, point(90.0, -60.0));
    assert_transform(&frame(&mut tracker), 2.0, point(90.0, -60.0));
    assert_eq!(
        tracker.remove_device(DEVICE),
        vec![(DEVICE.to_owned(), TouchAction::EndPinch { cancelled: true })]
    );
    assert!(tracker.remove_device(DEVICE).is_empty());
    assert!(!tracker.has_contacts());
    begin(&mut tracker);
    transform(&mut tracker, 1.0, point(-30.0, 30.0));
    assert_transform(&frame(&mut tracker), 1.0, point(-30.0, 30.0));
}
