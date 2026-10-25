//! Two contacts share the original surface grab, independently of lift order.
use super::*;

fn native_pair(scale: f64, first_lift: i32) {
    let _guard = headless_test_guard();
    let inset = (12, 8);
    let (compositor, mut client) = fixture(inset, scale);
    client.enable_touch();
    let viewport = query(&compositor, "status")["viewport"].clone();
    down(&compositor, 0, screen(&compositor, (40.25, 30.5)));
    frame(&compositor);
    let first_serial = assert_down(&mut client, 0, (52.25, 38.5));
    down(&compositor, 1, screen(&compositor, (140.5, 70.25)));
    frame(&compositor);
    let second_serial = assert_down(&mut client, 1, (152.5, 78.25));
    assert_ne!(first_serial, second_serial);

    motion(&compositor, 0, screen(&compositor, (60.75, 45.25)));
    motion(&compositor, 1, screen(&compositor, (160.25, 90.5)));
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [
            Event::Motion { id: 0, x: x0, y: y0, .. },
            Event::Motion { id: 1, x: x1, y: y1, .. }, Event::Frame,
        ] if (*x0, *y0) == (72.75, 53.25) && (*x1, *y1) == (172.25, 98.5)),
        "both contacts need exact unscaled surface coordinates: {events:?}",
    );
    assert_no_pointer_buttons(&mut client);

    // Reuse the second slot before the first lifts, within one device frame.
    up(&compositor, 1);
    down(&compositor, 1, screen(&compositor, (130.5, 80.25)));
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [
            Event::Up { id: 1, .. },
            Event::Down { surface, id: 1, x, y, .. }, Event::Frame,
        ] if surface == client.surface(0) && (*x, *y) == (142.5, 88.25)),
        "reusing a lifted slot must not count as a third live contact: {events:?}",
    );
    assert_ne!(client.touch_down_serial(), second_serial);
    motion(&compositor, 1, screen(&compositor, (150.75, 100.5)));
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [Event::Motion { id: 1, x, y, .. }, Event::Frame]
            if (*x, *y) == (162.75, 108.5)),
        "the reused slot must keep receiving motion: {events:?}",
    );

    up(&compositor, first_lift);
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [Event::Up { id, .. }, Event::Frame] if *id == first_lift),
        "each contact must lift exactly once: {events:?}",
    );
    let remaining = 1 - first_lift;
    motion(&compositor, remaining, screen(&compositor, (90.5, 65.75)));
    up(&compositor, remaining);
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [
            Event::Motion { id: moved, x, y, .. }, Event::Up { id: lifted, .. }, Event::Frame,
        ] if *moved == remaining && *lifted == remaining && (*x, *y) == (102.5, 73.75)),
        "surviving contact must retain final motion and lift: {events:?}",
    );
    assert_no_pointer_buttons(&mut client);
    for field in ["scale", "x", "y", "pinch_active"] {
        assert_eq!(
            query(&compositor, "status")["viewport"][field],
            viewport[field],
            "two native contacts must not navigate the canvas",
        );
    }
    motion(&compositor, 0, screen(&compositor, (200.0, 120.0)));
    motion(&compositor, 1, screen(&compositor, (210.0, 130.0)));
    up(&compositor, 0);
    up(&compositor, 1);
    frame(&compositor);
    assert_touch_silent(&mut client);
    fresh_tap(&compositor, &mut client, inset);
}

#[test]
fn two_contacts_with_insets_at_scale_one_original_lifts_first() {
    native_pair(1.0, 0);
}

#[test]
fn two_contacts_with_insets_at_scale_one_second_lifts_first() {
    native_pair(1.0, 1);
}

#[test]
fn two_contacts_with_insets_at_scale_half_original_lifts_first() {
    native_pair(0.5, 0);
}

#[test]
fn two_contacts_with_insets_at_scale_half_second_lifts_first() {
    native_pair(0.5, 1);
}

#[test]
fn second_contact_over_another_window_keeps_original_surface_and_transform() {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture((12, 8), 0.5);
    client.enable_touch();
    down(&compositor, 0, screen(&compositor, (40.0, 30.0)));
    frame(&compositor);
    assert_down(&mut client, 0, (52.0, 38.0));
    let sibling = query(&compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["title"] == "Interactive fixture 1")
        .unwrap()
        .clone();
    let second = (
        sibling["x"].as_f64().unwrap() + 20.0,
        sibling["y"].as_f64().unwrap() + 15.0,
    );
    let origin = screen(&compositor, (0.0, 0.0));
    let local = (
        (second.0 - origin.0) / 0.5 + 12.0,
        (second.1 - origin.1) / 0.5 + 8.0,
    );
    down(&compositor, 1, second);
    frame(&compositor);
    assert_down(&mut client, 1, local);
    assert_eq!(window(&compositor)["focused"], true);
    up(&compositor, 0);
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(matches!(
        events.as_slice(),
        [Event::Up { id: 0, .. }, Event::Frame]
    ));
    motion(&compositor, 1, (second.0 + 10.0, second.1 + 5.0));
    up(&compositor, 1);
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [
            Event::Motion { id: 1, x, y, .. }, Event::Up { id: 1, .. }, Event::Frame,
        ] if (*x, *y) == (local.0 + 20.0, local.1 + 10.0)),
        "remaining contact must retain the first surface's inset and scale: {events:?}",
    );
    assert_no_pointer_buttons(&mut client);
    fresh_tap(&compositor, &mut client, (12, 8));
}

#[test]
fn another_device_reusing_live_slots_cannot_steal_or_end_native_contacts() {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture((0, 0), 1.0);
    client.enable_touch();
    down(&compositor, 0, (140.0, 180.0));
    frame(&compositor);
    assert_down(&mut client, 0, (40.0, 30.0));
    down(&compositor, 1, (240.0, 210.0));
    frame(&compositor);
    assert_down(&mut client, 1, (140.0, 60.0));
    for command in [
        "touch-down foreign-touch 0 940 180",
        "touch-down foreign-touch 1 960 200",
        "touch-frame foreign-touch",
        "touch-motion foreign-touch 0 1000 220",
        "touch-motion foreign-touch 1 1020 240",
        "touch-frame foreign-touch",
        "touch-up foreign-touch 0",
        "touch-up foreign-touch 1",
        "touch-cancel foreign-touch",
        "touch-remove foreign-touch",
        "touch-frame foreign-touch",
    ] {
        action(&compositor, command);
        assert_touch_silent(&mut client);
    }
    motion(&compositor, 0, (160.0, 190.0));
    motion(&compositor, 1, (260.0, 220.0));
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [
            Event::Motion { id: 0, x: x0, y: y0, .. },
            Event::Motion { id: 1, x: x1, y: y1, .. }, Event::Frame,
        ] if (*x0, *y0) == (60.0, 40.0) && (*x1, *y1) == (160.0, 70.0)),
        "foreign slots must not corrupt either live contact: {events:?}",
    );
    up(&compositor, 1);
    up(&compositor, 0);
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(matches!(
        events.as_slice(),
        [
            Event::Up { id: 1, .. },
            Event::Up { id: 0, .. },
            Event::Frame,
        ]
    ));
    assert_no_pointer_buttons(&mut client);
    fresh_tap(&compositor, &mut client, (0, 0));
}
