//! Contact cancellation must end client ownership without stranding later input.
use super::*;

#[test]
fn third_contact_cancels_once_before_canvas_navigation() {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture((0, 0), 1.0);
    client.enable_touch();
    let viewport = query(&compositor, "status")["viewport"].clone();
    down(&compositor, 0, (140.0, 180.0));
    frame(&compositor);
    assert_down(&mut client, 0, (40.0, 30.0));

    down(&compositor, 1, (240.0, 180.0));
    frame(&compositor);
    assert_down(&mut client, 1, (140.0, 30.0));
    assert_no_pointer_buttons(&mut client);
    let second = query(&compositor, "status")["viewport"].clone();
    for field in ["scale", "x", "y"] {
        assert_eq!(second[field], viewport[field]);
    }
    assert_eq!(second["pinch_active"], false);

    down(&compositor, 2, (190.0, 280.0));
    // The cancellation must arrive before the frame recognizes navigation;
    // no third client down or synthesized up may escape in between.
    assert_cancel(&mut client);
    frame(&compositor);
    assert_touch_silent(&mut client);
    assert_eq!(
        query(&compositor, "status")["viewport"]["pinch_active"],
        true,
        "native delivery must preserve the compositor's three-finger gesture",
    );
    motion(&compositor, 0, (120.0, 160.0));
    motion(&compositor, 1, (260.0, 160.0));
    motion(&compositor, 2, (190.0, 300.0));
    frame(&compositor);
    assert_ne!(
        query(&compositor, "status")["viewport"]["scale"],
        viewport["scale"],
        "three-finger spread must actually zoom the canvas",
    );
    assert_touch_silent(&mut client);

    up(&compositor, 2);
    up(&compositor, 1);
    frame(&compositor);
    // The remaining contact must not resume delivery after the gesture ends.
    motion(&compositor, 0, (160.0, 210.0));
    frame(&compositor);
    up(&compositor, 0);
    frame(&compositor);
    assert_touch_silent(&mut client);
    assert_eq!(
        query(&compositor, "status")["viewport"]["pinch_active"],
        false,
    );
    action(&compositor, "zoom-to 1");
    wait_scale(&compositor, 1.0);
    action(&compositor, "pan-to 0 0");
    fresh_tap(&compositor, &mut client, (0, 0));
}

fn interrupted(command: &str, second_contact: bool, lift_original: bool) {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture((0, 0), 1.0);
    client.enable_touch();
    down(&compositor, 0, (140.0, 180.0));
    frame(&compositor);
    assert_down(&mut client, 0, (40.0, 30.0));
    if second_contact {
        down(&compositor, 1, (240.0, 210.0));
        frame(&compositor);
        assert_down(&mut client, 1, (140.0, 60.0));
    }
    if lift_original {
        up(&compositor, 0);
        frame(&compositor);
        let events = client.take_touch_events();
        assert!(matches!(
            events.as_slice(),
            [Event::Up { id: 0, .. }, Event::Frame]
        ));
    }
    assert_no_pointer_buttons(&mut client);

    // No motion: down/frame/cancel must work after Smithay consumed the frame.
    action(&compositor, command);
    frame(&compositor);
    assert_cancel(&mut client);
    action(&compositor, command);
    frame(&compositor);
    assert_touch_silent(&mut client);
    let viewport = query(&compositor, "status")["viewport"].clone();
    motion(&compositor, 0, (220.0, 250.0));
    motion(&compositor, 1, (260.0, 260.0));
    frame(&compositor);
    up(&compositor, 0);
    up(&compositor, 1);
    frame(&compositor);
    assert_touch_silent(&mut client);
    for field in ["scale", "x", "y"] {
        assert_eq!(
            query(&compositor, "status")["viewport"][field],
            viewport[field]
        );
    }
    // The same device and slot become usable again only with a fresh down.
    fresh_tap(&compositor, &mut client, (0, 0));
}

#[test]
fn touch_cancel_suppresses_stale_events_and_allows_next_sequence() {
    interrupted("touch-cancel client-touch", false, false);
}

#[test]
fn touch_device_removal_suppresses_stale_events_and_allows_next_sequence() {
    interrupted("touch-remove client-touch", false, false);
}

#[test]
fn two_contacts_cancel_once_suppress_stale_events_and_recover() {
    interrupted("touch-cancel client-touch", true, false);
}

#[test]
fn two_contacts_device_removal_cancels_once_and_recovers() {
    interrupted("touch-remove client-touch", true, false);
}

#[test]
fn cancel_remaining_second_contact_after_original_lift_recovers() {
    interrupted("touch-cancel client-touch 1", true, true);
}

#[test]
fn remove_device_with_only_second_contact_remaining_recovers() {
    interrupted("touch-remove client-touch", true, true);
}
