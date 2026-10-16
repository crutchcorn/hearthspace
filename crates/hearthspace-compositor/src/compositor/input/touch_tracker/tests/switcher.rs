use super::overview::{begin_overview, contacts};
use super::*;

fn transform(tracker: &mut TouchTracker, fingers: u32, scale: f64, translation: Position) {
    for (id, position) in contacts(fingers).into_iter().enumerate() {
        let transformed =
            point(200.0, 200.0) + (position - point(200.0, 200.0)).upscale(scale) + translation;
        tracker.motion(DEVICE, slot(id as u32), transformed);
    }
}

// Centroid/radius arithmetic varies slightly with contact iteration order.
// Match the entire action sequence exactly except for calculated coordinates.
fn assert_gesture_actions(actual: Vec<TouchAction>, expected: Vec<TouchAction>) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (actual, expected) in actual.iter().zip(&expected) {
        match (actual, expected) {
            (
                TouchAction::UpdateSwitcher {
                    translation: actual,
                },
                TouchAction::UpdateSwitcher {
                    translation: expected,
                },
            ) => {
                assert!(
                    (actual.x - expected.x).abs() < 1e-12,
                    "{actual:?} != {expected:?}"
                );
                assert!(
                    (actual.y - expected.y).abs() < 1e-12,
                    "{actual:?} != {expected:?}"
                );
            }
            (
                TouchAction::UpdateOverview { scale: actual },
                TouchAction::UpdateOverview { scale: expected },
            ) => assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}"),
            _ => assert_eq!(actual, expected),
        }
    }
}

fn begin_swipe(tracker: &mut TouchTracker, fingers: u32) {
    begin_overview(tracker, fingers);
    transform(tracker, fingers, 1.0, point(-40.0, 0.0));
    assert_gesture_actions(
        frame(tracker),
        vec![
            TouchAction::EndOverview { cancelled: true },
            TouchAction::BeginSwitcher { fingers },
            TouchAction::UpdateSwitcher {
                translation: point(-40.0, 0.0),
            },
        ],
    );
}

#[test]
fn four_and_five_finger_swipes_use_absolute_centroid_translation() {
    for fingers in [4, 5] {
        let mut tracker = TouchTracker::default();
        begin_swipe(&mut tracker, fingers);
        for x in [-120.0, 50.0, -30.0] {
            transform(&mut tracker, fingers, 1.0, point(x, 0.0));
            assert_gesture_actions(
                frame(&mut tracker),
                vec![TouchAction::UpdateSwitcher {
                    translation: point(x, 0.0),
                }],
            );
        }
        tracker.up(DEVICE, slot(0));
        assert_eq!(
            frame(&mut tracker),
            vec![TouchAction::EndSwitcher { cancelled: false }]
        );
        for id in 1..fingers {
            tracker.motion(DEVICE, slot(id), point(700.0, 900.0));
            tracker.up(DEVICE, slot(id));
        }
        assert!(
            frame(&mut tracker).is_empty(),
            "remaining contacts cannot click or pan"
        );
        begin_swipe(&mut tracker, fingers);
    }
}

#[test]
fn first_meaningful_intent_locks_pinch_or_swipe() {
    for fingers in [4, 5] {
        let mut tracker = TouchTracker::default();
        begin_overview(&mut tracker, fingers);
        transform(&mut tracker, fingers, 0.8, point(-40.0, 0.0));
        assert!(matches!(
            frame(&mut tracker).as_slice(),
            [TouchAction::UpdateOverview { .. }]
        ));
        transform(&mut tracker, fingers, 1.0, point(-200.0, 0.0));
        assert_gesture_actions(
            frame(&mut tracker),
            vec![TouchAction::UpdateOverview { scale: 1.0 }],
        );
        let mut tracker = TouchTracker::default();
        begin_swipe(&mut tracker, fingers);
        transform(&mut tracker, fingers, 0.5, point(-80.0, 0.0));
        assert_gesture_actions(
            frame(&mut tracker),
            vec![TouchAction::UpdateSwitcher {
                translation: point(-80.0, 0.0),
            }],
        );
    }
}

#[test]
fn jitter_and_vertical_motion_cannot_accidentally_switch_windows() {
    let mut tracker = TouchTracker::default();
    begin_overview(&mut tracker, 4);
    transform(&mut tracker, 4, 1.0, point(-31.0, 10.0));
    assert_gesture_actions(
        frame(&mut tracker),
        vec![TouchAction::UpdateOverview { scale: 1.0 }],
    );
    transform(&mut tracker, 4, 1.0, point(20.0, 50.0));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::EndOverview { cancelled: true }]
    );
    transform(&mut tracker, 4, 0.5, point(-200.0, 0.0));
    assert!(frame(&mut tracker).is_empty());
}

#[test]
fn fifth_contact_cancels_swipe_and_rebases_without_placement_motion() {
    let mut tracker = TouchTracker::default();
    begin_swipe(&mut tracker, 4);
    tracker.down(DEVICE, slot(4), point(900.0, 900.0));
    assert_eq!(
        frame(&mut tracker),
        vec![
            TouchAction::EndSwitcher { cancelled: true },
            TouchAction::BeginOverview { fingers: 5 },
        ]
    );
    let positions: Vec<_> = tracker
        .contacts
        .iter()
        .map(|(&id, contact)| (id, contact.position))
        .collect();
    for (id, position) in positions {
        tracker.motion(DEVICE, id, position + point(-40.0, 0.0));
    }
    assert_gesture_actions(
        frame(&mut tracker),
        vec![
            TouchAction::EndOverview { cancelled: true },
            TouchAction::BeginSwitcher { fingers: 5 },
            TouchAction::UpdateSwitcher {
                translation: point(-40.0, 0.0),
            },
        ],
    );
}

#[test]
fn cancellation_reset_removal_and_interrupt_cannot_commit_or_restart() {
    for cleanup in 0..4 {
        let mut tracker = TouchTracker::default();
        begin_swipe(&mut tracker, 4);
        let actions = match cleanup {
            0 => {
                tracker.cancel(DEVICE, slot(0));
                tracker.frame(DEVICE)
            }
            1 => tracker.remove_device(DEVICE),
            2 => tracker.reset(),
            _ => tracker.interrupt(),
        };
        assert_eq!(
            actions,
            vec![(
                DEVICE.to_owned(),
                TouchAction::EndSwitcher { cancelled: true }
            )]
        );
        for id in 0..4 {
            tracker.motion(DEVICE, slot(id), point(900.0, 900.0));
            tracker.up(DEVICE, slot(id));
        }
        assert!(frame(&mut tracker).is_empty());
    }
}

#[test]
fn reset_and_device_removal_cancel_an_end_queued_before_dispatch() {
    for remove in [false, true] {
        let mut tracker = TouchTracker::default();
        begin_swipe(&mut tracker, 5);
        for id in 0..5 {
            tracker.up(DEVICE, slot(id));
        }
        let actions = if remove {
            tracker.remove_device(DEVICE)
        } else {
            tracker.reset()
        };
        assert_eq!(
            actions,
            vec![(
                DEVICE.to_owned(),
                TouchAction::EndSwitcher { cancelled: true }
            )]
        );
    }
}

#[test]
fn final_swipe_motion_precedes_end_in_same_frame() {
    let mut tracker = TouchTracker::default();
    begin_swipe(&mut tracker, 4);
    transform(&mut tracker, 4, 1.0, point(-130.0, 0.0));
    tracker.up(DEVICE, slot(0));
    assert_gesture_actions(
        frame(&mut tracker),
        vec![
            TouchAction::UpdateSwitcher {
                translation: point(-130.0, 0.0),
            },
            TouchAction::EndSwitcher { cancelled: false },
        ],
    );
}

#[test]
fn sixth_contact_and_cancel_after_first_lift_reject_queued_commit() {
    let mut tracker = TouchTracker::default();
    begin_swipe(&mut tracker, 5);
    tracker.down(DEVICE, slot(5), point(900.0, 900.0));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::EndSwitcher { cancelled: true }]
    );
    let mut tracker = TouchTracker::default();
    begin_swipe(&mut tracker, 4);
    tracker.up(DEVICE, slot(0));
    tracker.cancel(DEVICE, slot(1));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::EndSwitcher { cancelled: true }]
    );
}

#[test]
fn suppressing_a_fresh_contact_prevents_late_tap_after_modal_dismissal() {
    let mut tracker = TouchTracker::default();
    tracker.down(DEVICE, slot(0), point(200.0, 200.0));
    assert!(tracker.interrupt().is_empty());
    tracker.motion(DEVICE, slot(0), point(400.0, 200.0));
    tracker.up(DEVICE, slot(0));
    assert!(frame(&mut tracker).is_empty());
    tracker.down(DEVICE, slot(0), point(200.0, 200.0));
    tracker.up(DEVICE, slot(0));
    assert_eq!(
        frame(&mut tracker),
        vec![
            TouchAction::Hover(point(200.0, 200.0)),
            TouchAction::Tap(point(200.0, 200.0)),
        ]
    );
}
