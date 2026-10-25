//! A multi-contact sequence never regains CSD move/resize authority mid-stream.
use super::*;

fn rejected_operation(operation: Operation, lifted: Option<i32>) {
    let _guard = headless_test_guard();
    let inset = (12, 8);
    let (compositor, mut client) = fixture(inset, 0.5);
    client.enable_touch();
    let before = geometries(&compositor);
    down(&compositor, 0, screen(&compositor, (40.0, 30.0)));
    frame(&compositor);
    let original = assert_down(&mut client, 0, (52.0, 38.0));
    down(&compositor, 1, screen(&compositor, (140.0, 60.0)));
    frame(&compositor);
    let second = assert_down(&mut client, 1, (152.0, 68.0));
    assert_ne!(original, second);
    if let Some(slot) = lifted {
        up(&compositor, slot);
        frame(&compositor);
        let events = client.take_touch_events();
        assert!(
            matches!(events.as_slice(), [Event::Up { id, .. }, Event::Frame] if *id == slot),
            "lifting either finger must preserve the remaining contact: {events:?}",
        );
    }

    // No earlier rejected request can consume authority and mask revival on lift.
    let remaining = if lifted == Some(0) { 1 } else { 0 };
    for (serial, point) in [(original, (80.0, 50.0)), (second, (100.0, 70.0))] {
        operation.request(&mut client, 0, serial);
        motion(&compositor, remaining, screen(&compositor, point));
        frame(&compositor);
        client.sync();
        assert_eq!(
            geometries(&compositor),
            before,
            "{operation:?} accepted a multi-contact serial after lift {lifted:?}",
        );
        assert!(
            !client.configuration(0).1,
            "rejected resize entered resizing state"
        );
        let events = client.take_touch_events();
        assert!(
            matches!(events.as_slice(), [Event::Motion { id, x, y, .. }, Event::Frame]
                if *id == remaining && (*x, *y) == (point.0 + 12.0, point.1 + 8.0)),
            "rejected {operation:?} must preserve native motion without cancellation: {events:?}",
        );
        assert_no_pointer_buttons(&mut client);
    }
    for slot in [0, 1] {
        if lifted == Some(slot) {
            continue;
        }
        up(&compositor, slot);
        frame(&compositor);
        let events = client.take_touch_events();
        assert!(matches!(events.as_slice(), [Event::Up { id, .. }, Event::Frame] if *id == slot));
    }
    // Neither serial can attach a late operation to an unrelated mouse motion.
    for serial in [original, second] {
        operation.request(&mut client, 0, serial);
    }
    action(&compositor, "pointer-motion-abs 1100 700");
    client.sync();
    assert_eq!(geometries(&compositor), before);
    assert_touch_silent(&mut client);

    // Recovery must authorize the actual operation, not merely deliver a tap.
    let start = screen(&compositor, (40.0, 30.0));
    down(&compositor, 0, start);
    frame(&compositor);
    let fresh = assert_down(&mut client, 0, (52.0, 38.0));
    assert_ne!(fresh, original);
    assert_ne!(fresh, second);
    operation.request(&mut client, 0, fresh);
    assert_cancel(&mut client);
    motion(&compositor, 0, (start.0 + 16.0, start.1 + 8.0));
    frame(&compositor);
    assert_geometry(
        &compositor,
        &mut client,
        operation.geometry(before[0], (32, 16)),
    );
    // Resize advertises its active state only once motion changes geometry.
    assert_eq!(
        client.configuration(0).1,
        matches!(operation, Operation::Resize)
    );
    motion(&compositor, 0, (start.0 + 32.0, start.1 + 16.0));
    up(&compositor, 0);
    frame(&compositor);
    let expected = operation.geometry(before[0], (64, 32));
    assert_geometry(&compositor, &mut client, expected);
    assert!(
        !client.configuration(0).1,
        "fresh resize must finish on lift"
    );
    assert_touch_silent(&mut client);
    motion(&compositor, 0, (start.0 + 48.0, start.1 + 24.0));
    frame(&compositor);
    assert_geometry(&compositor, &mut client, expected);
    assert_touch_silent(&mut client);
}

#[test]
fn native_move_rejects_two_contacts_then_fresh_single_contact_works() {
    rejected_operation(Operation::Move, None);
}

#[test]
fn native_resize_rejects_two_contacts_then_fresh_single_contact_works() {
    rejected_operation(Operation::Resize, None);
}

#[test]
fn native_move_stays_rejected_after_original_contact_lifts() {
    rejected_operation(Operation::Move, Some(0));
}

#[test]
fn native_resize_stays_rejected_after_original_contact_lifts() {
    rejected_operation(Operation::Resize, Some(0));
}

#[test]
fn native_move_stays_rejected_after_second_contact_lifts() {
    rejected_operation(Operation::Move, Some(1));
}

#[test]
fn native_resize_stays_rejected_after_second_contact_lifts() {
    rejected_operation(Operation::Resize, Some(1));
}
