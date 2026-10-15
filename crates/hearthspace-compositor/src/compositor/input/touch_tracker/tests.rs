use super::*;

mod pan;

const DEVICE: &str = "screen";

fn slot(id: u32) -> TouchSlot {
    Some(id).into()
}

fn point(x: f64, y: f64) -> Position {
    (x, y).into()
}

fn frame(tracker: &mut TouchTracker) -> Vec<TouchAction> {
    tracker
        .frame(DEVICE)
        .into_iter()
        .map(|(_, action)| action)
        .collect()
}

fn triangle(tracker: &mut TouchTracker) {
    for (id, position) in [
        (0, point(0.0, 0.0)),
        (1, point(60.0, 0.0)),
        (2, point(30.0, 60.0)),
    ] {
        tracker.down(DEVICE, slot(id), position);
    }
}

fn begin(tracker: &mut TouchTracker) {
    triangle(tracker);
    assert_eq!(
        frame(tracker),
        vec![
            TouchAction::Hover(point(0.0, 0.0)),
            TouchAction::BeginPinch {
                anchor: point(30.0, 20.0)
            }
        ]
    );
}

fn assert_scale(actions: Vec<TouchAction>, expected: f64) {
    let [TouchAction::UpdatePinch { scale, .. }] = actions.as_slice() else {
        panic!("unexpected actions: {actions:?}");
    };
    assert!((scale - expected).abs() < 1e-12, "{scale} != {expected}");
}

#[test]
fn tap_is_delayed_until_up_and_small_motion_does_not_press() {
    let mut tracker = TouchTracker::default();
    tracker.down(DEVICE, slot(0), point(20.0, 40.0));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::Hover(point(20.0, 40.0))]
    );
    tracker.motion(DEVICE, slot(0), point(23.0, 44.0));
    assert!(frame(&mut tracker).is_empty());
    tracker.up(DEVICE, slot(0));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::Tap(point(23.0, 44.0))]
    );
    assert!(!tracker.has_contacts());
}

#[test]
fn drag_presses_at_origin_then_moves_and_releases_once() {
    let mut tracker = TouchTracker::default();
    tracker.down(DEVICE, slot(0), point(20.0, 40.0));
    frame(&mut tracker);
    tracker.motion(DEVICE, slot(0), point(28.0, 40.0));
    assert_eq!(
        frame(&mut tracker),
        vec![
            TouchAction::Press(point(20.0, 40.0)),
            TouchAction::Motion(point(28.0, 40.0))
        ]
    );
    tracker.motion(DEVICE, slot(0), point(30.0, 43.0));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::Motion(point(30.0, 43.0))]
    );
    tracker.up(DEVICE, slot(0));
    assert_eq!(frame(&mut tracker), vec![TouchAction::Release]);
    tracker.up(DEVICE, slot(0));
    assert!(frame(&mut tracker).is_empty());
}

#[test]
fn drag_motion_and_release_in_one_frame_preserve_order() {
    let mut tracker = TouchTracker::default();
    tracker.down(DEVICE, slot(0), point(0.0, 0.0));
    tracker.motion(DEVICE, slot(0), point(10.0, 10.0));
    tracker.up(DEVICE, slot(0));
    tracker.down(DEVICE, slot(0), point(100.0, 100.0));
    assert_eq!(
        frame(&mut tracker),
        vec![
            TouchAction::Hover(point(0.0, 0.0)),
            TouchAction::Press(point(0.0, 0.0)),
            TouchAction::Motion(point(10.0, 10.0)),
            TouchAction::Release,
            TouchAction::Hover(point(100.0, 100.0))
        ]
    );
}

#[test]
fn two_contacts_never_click_or_resume_a_single_finger_drag() {
    let mut tracker = TouchTracker::default();
    tracker.down(DEVICE, slot(0), point(0.0, 0.0));
    tracker.down(DEVICE, slot(1), point(60.0, 0.0));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::Hover(point(0.0, 0.0))]
    );
    tracker.up(DEVICE, slot(1));
    tracker.motion(DEVICE, slot(0), point(100.0, 100.0));
    tracker.up(DEVICE, slot(0));
    assert!(frame(&mut tracker).is_empty());
}

#[test]
fn extra_fingers_end_committed_drag_and_suppress_zoom() {
    let mut tracker = TouchTracker::default();
    tracker.down(DEVICE, slot(0), point(0.0, 0.0));
    tracker.motion(DEVICE, slot(0), point(20.0, 20.0));
    frame(&mut tracker);
    tracker.down(DEVICE, slot(1), point(60.0, 0.0));
    tracker.down(DEVICE, slot(2), point(30.0, 60.0));
    assert_eq!(frame(&mut tracker), vec![TouchAction::Release]);
    tracker.motion(DEVICE, slot(1), point(120.0, 0.0));
    assert!(frame(&mut tracker).is_empty());
    for id in 0..3 {
        tracker.up(DEVICE, slot(id));
    }
    assert!(frame(&mut tracker).is_empty());
}

#[test]
fn pinch_uses_frame_geometry_and_absolute_radius_ratio() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    tracker.motion(DEVICE, slot(0), point(-30.0, -20.0));
    tracker.motion(DEVICE, slot(1), point(90.0, -20.0));
    tracker.motion(DEVICE, slot(2), point(30.0, 100.0));
    assert_scale(frame(&mut tracker), 2.0);
    tracker.motion(DEVICE, slot(0), point(-60.0, -40.0));
    tracker.motion(DEVICE, slot(1), point(120.0, -40.0));
    tracker.motion(DEVICE, slot(2), point(30.0, 140.0));
    assert_scale(frame(&mut tracker), 3.0);
    assert!(frame(&mut tracker).is_empty());
}

#[test]
fn translation_and_rotation_do_not_change_scale() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    // Rotate the triangle 90 degrees around its centroid and translate it.
    for (id, position) in [
        (0, point(150.0, 190.0)),
        (1, point(150.0, 250.0)),
        (2, point(90.0, 220.0)),
    ] {
        tracker.motion(DEVICE, slot(id), position);
    }
    assert_scale(frame(&mut tracker), 1.0);
}

#[test]
fn fourth_contact_cancels_even_when_it_lifts_before_frame() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    tracker.down(DEVICE, slot(3), point(100.0, 100.0));
    tracker.up(DEVICE, slot(3));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::EndPinch { cancelled: true }]
    );
    tracker.motion(DEVICE, slot(1), point(120.0, 0.0));
    assert!(frame(&mut tracker).is_empty());
    for id in 0..3 {
        tracker.up(DEVICE, slot(id));
    }
    assert!(frame(&mut tracker).is_empty());
    begin(&mut tracker);
}

#[test]
fn four_contacts_before_first_frame_cannot_start_after_one_lifts() {
    let mut tracker = TouchTracker::default();
    triangle(&mut tracker);
    tracker.down(DEVICE, slot(3), point(100.0, 100.0));
    tracker.up(DEVICE, slot(3));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::Hover(point(0.0, 0.0))]
    );
}

#[test]
fn normal_lift_ends_pinch_and_cannot_restart_before_all_up() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    tracker.up(DEVICE, slot(2));
    tracker.down(DEVICE, slot(2), point(30.0, 60.0));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::EndPinch { cancelled: false }]
    );
    for id in 0..3 {
        tracker.up(DEVICE, slot(id));
    }
    triangle(&mut tracker);
    assert_eq!(
        frame(&mut tracker),
        vec![
            TouchAction::Hover(point(0.0, 0.0)),
            TouchAction::BeginPinch {
                anchor: point(30.0, 20.0)
            }
        ]
    );
}

#[test]
fn cancellation_is_scoped_and_remaining_contacts_stay_suppressed() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    tracker.cancel("other", slot(0));
    tracker.cancel(DEVICE, slot(99));
    assert!(frame(&mut tracker).is_empty());
    tracker.cancel(DEVICE, slot(0));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::EndPinch { cancelled: true }]
    );
    assert!(tracker.has_contacts());
    tracker.down(DEVICE, slot(0), point(0.0, 0.0));
    assert!(frame(&mut tracker).is_empty());
    tracker.cancel(DEVICE, None.into());
    assert!(!tracker.has_contacts());
    assert!(frame(&mut tracker).is_empty());
    begin(&mut tracker);
}

#[test]
fn devices_and_unknown_slots_cannot_contribute_to_owned_gesture() {
    let mut tracker = TouchTracker::default();
    tracker.down(DEVICE, slot(0), point(0.0, 0.0));
    tracker.down("other", slot(1), point(60.0, 0.0));
    tracker.down("other", slot(2), point(30.0, 60.0));
    tracker.motion("other", slot(0), point(100.0, 100.0));
    tracker.motion(DEVICE, slot(99), point(100.0, 100.0));
    tracker.up("other", slot(0));
    tracker.up(DEVICE, slot(99));
    assert!(tracker.frame("other").is_empty());
    assert!(tracker.remove_device("other").is_empty());
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::Hover(point(0.0, 0.0))]
    );
    tracker.up(DEVICE, slot(0));
    assert_eq!(frame(&mut tracker), vec![TouchAction::Tap(point(0.0, 0.0))]);
}

#[test]
fn all_up_and_new_device_before_frame_preserve_old_end_device() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    for id in 0..3 {
        tracker.up(DEVICE, slot(id));
    }
    tracker.down("other", slot(0), point(40.0, 40.0));
    assert_eq!(
        tracker.frame_all(),
        vec![
            (
                DEVICE.to_owned(),
                TouchAction::EndPinch { cancelled: false }
            ),
            ("other".to_owned(), TouchAction::Hover(point(40.0, 40.0)))
        ]
    );
}

#[test]
fn degenerate_and_nonfinite_geometry_never_emit_invalid_zoom() {
    let mut tracker = TouchTracker::default();
    for id in 0..3 {
        tracker.down(DEVICE, slot(id), point(0.0, 0.0));
    }
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::Hover(point(0.0, 0.0))]
    );
    tracker.motion(DEVICE, slot(1), point(f64::NAN, 0.0));
    tracker.motion(DEVICE, slot(2), point(30.0, 60.0));
    assert!(frame(&mut tracker).is_empty());
    tracker.motion(DEVICE, slot(1), point(60.0, 0.0));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::BeginPinch {
            anchor: point(30.0, 20.0)
        }]
    );
    tracker.motion(DEVICE, slot(1), point(f64::INFINITY, 0.0));
    assert!(frame(&mut tracker).is_empty());
    tracker.motion(DEVICE, slot(1), point(60.0, 0.0));
    assert_scale(frame(&mut tracker), 1.0);
}

#[test]
fn interrupt_preserves_zoom_and_discards_late_cancellation_until_all_up() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    assert!(tracker.interrupt().is_empty());
    tracker.cancel(DEVICE, slot(0));
    assert!(frame(&mut tracker).is_empty());
    tracker.motion(DEVICE, slot(1), point(120.0, 0.0));
    assert!(frame(&mut tracker).is_empty());
    tracker.up(DEVICE, slot(1));
    tracker.up(DEVICE, slot(2));
    begin(&mut tracker);
    tracker.down(DEVICE, slot(3), point(100.0, 100.0));
    assert!(
        tracker.interrupt().is_empty(),
        "queued cancellation must not undo external input"
    );
}

#[test]
fn interruption_releases_drag_and_reset_cancels_pinch_and_discards_taps() {
    let mut tracker = TouchTracker::default();
    tracker.down(DEVICE, slot(0), point(0.0, 0.0));
    tracker.motion(DEVICE, slot(0), point(20.0, 20.0));
    frame(&mut tracker);
    assert_eq!(
        tracker.interrupt(),
        vec![(DEVICE.to_owned(), TouchAction::Release)]
    );
    tracker.up(DEVICE, slot(0));
    assert!(frame(&mut tracker).is_empty());
    begin(&mut tracker);
    assert_eq!(
        tracker.reset(),
        vec![(DEVICE.to_owned(), TouchAction::EndPinch { cancelled: true })]
    );
    assert!(!tracker.has_contacts());
    tracker.down(DEVICE, slot(0), point(0.0, 0.0));
    tracker.up(DEVICE, slot(0));
    assert!(tracker.reset().is_empty());
}

#[test]
fn device_removal_cancels_matching_gesture_and_clears_contacts() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    assert!(tracker.remove_device("other").is_empty());
    assert_eq!(
        tracker.remove_device(DEVICE),
        vec![(DEVICE.to_owned(), TouchAction::EndPinch { cancelled: true })]
    );
    assert!(!tracker.has_contacts());
    assert!(tracker.remove_device(DEVICE).is_empty());
    begin(&mut tracker);
}

#[test]
fn interrupt_without_contacts_does_not_suppress_next_gesture() {
    let mut tracker = TouchTracker::default();
    assert!(tracker.interrupt().is_empty());
    begin(&mut tracker);
}

#[test]
fn final_motion_before_up_is_applied_before_normal_end() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    tracker.motion(DEVICE, slot(0), point(-30.0, -20.0));
    tracker.motion(DEVICE, slot(1), point(90.0, -20.0));
    tracker.motion(DEVICE, slot(2), point(30.0, 100.0));
    tracker.up(DEVICE, slot(0));
    let mut actions = frame(&mut tracker);
    assert_eq!(
        actions.pop(),
        Some(TouchAction::EndPinch { cancelled: false })
    );
    assert_scale(actions, 2.0);
}
