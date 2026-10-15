use super::*;

fn contacts(fingers: u32) -> Vec<Position> {
    (0..fingers)
        .map(|index| {
            let angle = std::f64::consts::TAU * f64::from(index) / f64::from(fingers);
            point(200.0 + 100.0 * angle.cos(), 200.0 + 100.0 * angle.sin())
        })
        .collect()
}

fn begin_overview(tracker: &mut TouchTracker, fingers: u32) {
    for (id, position) in contacts(fingers).into_iter().enumerate() {
        tracker.down(DEVICE, slot(id as u32), position);
    }
    assert_eq!(
        frame(tracker),
        vec![
            TouchAction::Hover(point(300.0, 200.0)),
            TouchAction::BeginOverview { fingers },
        ]
    );
}

fn transform_contacts(tracker: &mut TouchTracker, scale: f64, rotate: bool) {
    let center = point(200.0, 200.0);
    let positions: Vec<_> = tracker
        .contacts
        .iter()
        .map(|(&slot, contact)| {
            let delta = contact.position - center;
            let delta = if rotate {
                point(-delta.y, delta.x)
            } else {
                delta
            };
            (slot, center + delta.upscale(scale) + point(90.0, -30.0))
        })
        .collect();
    for (slot, position) in positions {
        tracker.motion(DEVICE, slot, position);
    }
}

fn assert_overview_scale(actions: &[TouchAction], expected: f64) {
    let [TouchAction::UpdateOverview { scale }] = actions else {
        panic!("unexpected actions: {actions:?}");
    };
    assert!((scale - expected).abs() < 1e-12, "{scale} != {expected}");
}

#[test]
fn four_and_five_contacts_report_absolute_pinch_only() {
    for fingers in [4, 5] {
        let mut tracker = TouchTracker::default();
        begin_overview(&mut tracker, fingers);
        transform_contacts(&mut tracker, 0.7, true);
        assert_overview_scale(&frame(&mut tracker), 0.7);
        transform_contacts(&mut tracker, 2.0, false);
        assert_overview_scale(&frame(&mut tracker), 1.4);
        tracker.up(DEVICE, slot(0));
        assert_eq!(
            frame(&mut tracker),
            vec![TouchAction::EndOverview { cancelled: false }]
        );
        for id in 1..fingers {
            tracker.motion(DEVICE, slot(id), point(900.0, 900.0));
            tracker.up(DEVICE, slot(id));
        }
        assert!(
            frame(&mut tracker).is_empty(),
            "remaining fingers cannot pan or click"
        );
        begin_overview(&mut tracker, fingers);
    }
}

#[test]
fn translation_and_rotation_never_request_an_overview_transition() {
    for fingers in [4, 5] {
        let mut tracker = TouchTracker::default();
        begin_overview(&mut tracker, fingers);
        transform_contacts(&mut tracker, 1.0, true);
        assert_overview_scale(&frame(&mut tracker), 1.0);
    }
}

#[test]
fn late_fourth_contact_cancels_provisional_canvas_motion_and_promotes() {
    let mut tracker = TouchTracker::default();
    begin(&mut tracker);
    pan::transform(&mut tracker, 0.5, point(60.0, -30.0));
    assert_scale(frame(&mut tracker), 0.5);
    tracker.down(DEVICE, slot(3), point(100.0, 100.0));
    assert_eq!(
        frame(&mut tracker),
        vec![
            TouchAction::EndPinch { cancelled: true },
            TouchAction::BeginOverview { fingers: 4 },
        ]
    );
    transform_contacts(&mut tracker, 0.7, false);
    assert_overview_scale(&frame(&mut tracker), 0.7);
}

#[test]
fn fifth_contact_rebases_instead_of_counting_placement_as_expansion() {
    let mut tracker = TouchTracker::default();
    begin_overview(&mut tracker, 4);
    transform_contacts(&mut tracker, 0.7, false);
    assert_overview_scale(&frame(&mut tracker), 0.7);
    tracker.down(DEVICE, slot(4), point(900.0, 900.0));
    assert_eq!(
        frame(&mut tracker),
        vec![
            TouchAction::EndOverview { cancelled: true },
            TouchAction::BeginOverview { fingers: 5 },
        ]
    );
    transform_contacts(&mut tracker, 1.0, true);
    assert_overview_scale(&frame(&mut tracker), 1.0);
    transform_contacts(&mut tracker, 1.4, false);
    assert_overview_scale(&frame(&mut tracker), 1.4);
}

#[test]
fn sixth_contact_cancels_and_suppresses_until_all_fingers_lift() {
    let mut tracker = TouchTracker::default();
    begin_overview(&mut tracker, 5);
    transform_contacts(&mut tracker, 0.6, false);
    assert_overview_scale(&frame(&mut tracker), 0.6);
    tracker.down(DEVICE, slot(5), point(600.0, 300.0));
    tracker.up(DEVICE, slot(5));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::EndOverview { cancelled: true }]
    );
    for id in 0..5 {
        tracker.up(DEVICE, slot(id));
    }
    assert!(frame(&mut tracker).is_empty());
    begin_overview(&mut tracker, 4);
}

#[test]
fn all_six_contacts_before_first_frame_cannot_downgrade_to_five() {
    let mut tracker = TouchTracker::default();
    for (id, position) in contacts(6).into_iter().enumerate() {
        tracker.down(DEVICE, slot(id as u32), position);
    }
    tracker.up(DEVICE, slot(5));
    assert_eq!(
        frame(&mut tracker),
        vec![TouchAction::Hover(point(300.0, 200.0))]
    );
    transform_contacts(&mut tracker, 0.5, false);
    assert!(frame(&mut tracker).is_empty());
}

#[test]
fn final_motion_in_lift_frame_precedes_overview_commit() {
    for fingers in [4, 5] {
        let mut tracker = TouchTracker::default();
        begin_overview(&mut tracker, fingers);
        transform_contacts(&mut tracker, 0.6, false);
        tracker.up(DEVICE, slot(0));
        let mut actions = frame(&mut tracker);
        assert_eq!(
            actions.pop(),
            Some(TouchAction::EndOverview { cancelled: false })
        );
        assert_overview_scale(&actions, 0.6);
    }
}

#[test]
fn cancel_device_removal_reset_and_external_input_never_commit_overview() {
    for cleanup in 0..4 {
        let mut tracker = TouchTracker::default();
        begin_overview(&mut tracker, 4);
        transform_contacts(&mut tracker, 0.6, false);
        frame(&mut tracker);
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
                TouchAction::EndOverview { cancelled: true }
            )]
        );
        for id in 0..4 {
            tracker.up(DEVICE, slot(id));
        }
        assert!(frame(&mut tracker).is_empty());
    }
}

#[test]
fn pending_end_is_cancelled_if_reset_or_removal_arrives_before_dispatch() {
    for remove in [false, true] {
        let mut tracker = TouchTracker::default();
        begin_overview(&mut tracker, 5);
        transform_contacts(&mut tracker, 0.6, false);
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
                TouchAction::EndOverview { cancelled: true }
            )]
        );
    }
}

#[test]
fn cancellation_after_one_lift_cannot_commit_a_pending_end() {
    let mut tracker = TouchTracker::default();
    begin_overview(&mut tracker, 4);
    transform_contacts(&mut tracker, 0.6, false);
    tracker.up(DEVICE, slot(0));
    tracker.cancel(DEVICE, slot(1));
    let mut actions = frame(&mut tracker);
    assert_eq!(
        actions.pop(),
        Some(TouchAction::EndOverview { cancelled: true })
    );
    assert_overview_scale(&actions, 0.6);
}
