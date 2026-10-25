//! GTK titlebar authority comes from actual native touch, independent of pointer focus.
use super::*;
use crate::interactive::client::events::assert_buttons;
use wayland_client::protocol::wl_touch::Event;

fn native_fixture() -> (HeadlessCompositor, InteractiveClient, u64) {
    let (compositor, mut client, id) = fixture();
    client.enable_touch();
    // Leave the pointer outside both clients before generating native touch.
    action(&compositor, "pointer-motion-abs 1200 700");
    client.take_pointer_events();
    (compositor, client, id)
}

fn frame(compositor: &HeadlessCompositor) {
    action(compositor, "touch-frame gtk-native");
}

fn down(compositor: &HeadlessCompositor, client: &mut InteractiveClient, id: u64) -> u32 {
    let w = window(compositor, id);
    action(
        compositor,
        format!(
            "touch-down gtk-native 0 {} {}",
            w["x"].as_f64().unwrap() + 40.0,
            w["y"].as_f64().unwrap() + 20.0,
        ),
    );
    frame(compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [Event::Down { surface, id: 0, x, y, .. }, Event::Frame]
            if surface == client.surface(0) && (*x, *y) == (40.0, 20.0)),
        "GTK must receive a fresh native down: {events:?}",
    );
    assert_buttons(&client.take_pointer_events(), &[]);
    client.touch_down_serial()
}

fn up(compositor: &HeadlessCompositor) {
    action(compositor, "touch-up gtk-native 0");
    frame(compositor);
}

fn completed_tap(compositor: &HeadlessCompositor, client: &mut InteractiveClient, id: u64) -> u32 {
    let serial = down(compositor, client, id);
    up(compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [Event::Up { id: 0, .. }, Event::Frame]),
        "completed authority requires a real native up: {events:?}",
    );
    assert_buttons(&client.take_pointer_events(), &[]);
    serial
}

fn terminal_events(client: &mut InteractiveClient) -> Vec<Event> {
    let mut events = client.take_touch_events();
    events.retain(|event| !matches!(event, Event::Frame));
    assert_buttons(&client.take_pointer_events(), &[]);
    events
}

fn assert_cancel(client: &mut InteractiveClient) {
    let events = terminal_events(client);
    assert!(
        matches!(events.as_slice(), [Event::Cancel]),
        "native cancellation must end exactly once: {events:?}",
    );
}

fn fresh_gesture(compositor: &HeadlessCompositor, client: &mut InteractiveClient, id: u64) {
    let fresh = completed_tap(compositor, client, id);
    client.gtk_gesture(0, fresh, Gesture::DoubleClick);
    state(compositor, client, id, true, false);
}

#[test]
fn live_touch_titlebar_gesture_is_single_use_without_pointer_focus() {
    let _guard = headless_test_guard();
    let (compositor, mut client, id) = native_fixture();
    let sibling = query(&compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["title"] == "Interactive fixture 1")
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let serial = down(&compositor, &mut client, id);
    client.gtk_gesture(0, serial.wrapping_add(100_000), Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);
    client.gtk_gesture(1, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, sibling, false, false);
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, true, false);
    client.gtk_recreate_surface(0);
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, true, false);
    up(&compositor);
    // Reconfiguration may cancel the old geometry before the actual lift.
    let events = terminal_events(&mut client);
    assert!(
        matches!(
            events.as_slice(),
            [Event::Up { id: 0, .. }] | [Event::Cancel]
        ),
        "accepted native gesture must have one well-formed end: {events:?}",
    );
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, true, false);
    let fresh = completed_tap(&compositor, &mut client, id);
    client.gtk_gesture(0, fresh, Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);
}

#[test]
fn completed_touch_titlebar_gesture_rejects_replay_and_expiry() {
    let _guard = headless_test_guard();
    let (compositor, mut client, id) = native_fixture();
    let serial = completed_tap(&compositor, &mut client, id);
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, true, false);
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, true, false);
    client.gtk_request_maximized(0, false);
    state(&compositor, &mut client, id, false, false);

    let expired = completed_tap(&compositor, &mut client, id);
    thread::sleep(Duration::from_millis(1100));
    client.gtk_gesture(0, expired, Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);
    fresh_gesture(&compositor, &mut client, id);
}

fn cancelled_token(command: &str) {
    let _guard = headless_test_guard();
    let (compositor, mut client, id) = native_fixture();
    let serial = down(&compositor, &mut client, id);
    action(&compositor, command);
    frame(&compositor);
    assert_cancel(&mut client);
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);
    up(&compositor);
    action(&compositor, "touch-up gtk-native 1");
    frame(&compositor);
    assert!(terminal_events(&mut client).is_empty());
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);
    fresh_gesture(&compositor, &mut client, id);
}

#[test]
fn cancelled_touch_cannot_authorize_titlebar_gesture() {
    cancelled_token("touch-cancel gtk-native");
}

#[test]
fn removed_touch_device_cannot_authorize_titlebar_gesture() {
    cancelled_token("touch-remove gtk-native");
}

fn multi_contact_token(lifted: &[i32]) {
    let _guard = headless_test_guard();
    let (compositor, mut client, id) = native_fixture();
    let original = down(&compositor, &mut client, id);
    let w = window(&compositor, id);
    action(
        &compositor,
        format!(
            "touch-down gtk-native 1 {} {}",
            w["x"].as_f64().unwrap() + 140.0,
            w["y"].as_f64().unwrap() + 60.0
        ),
    );
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [Event::Down { surface, id: 1, x, y, .. }, Event::Frame]
            if surface == client.surface(0) && (*x, *y) == (140.0, 60.0)),
        "second native contact must arrive without cancellation: {events:?}",
    );
    let second = client.touch_down_serial();
    assert_ne!(original, second);
    for slot in lifted {
        action(&compositor, format!("touch-up gtk-native {slot}"));
        frame(&compositor);
        let events = terminal_events(&mut client);
        assert!(matches!(events.as_slice(), [Event::Up { id, .. }] if id == slot));
    }

    // First request occurs at this stage: an earlier rejection must not consume
    // the token and hide accidental authority revival after either finger lifts.
    for serial in [original, second] {
        client.gtk_gesture(0, serial, Gesture::DoubleClick);
        state(&compositor, &mut client, id, false, false);
    }
    assert!(terminal_events(&mut client).is_empty());
    for slot in [0, 1] {
        if lifted.contains(&slot) {
            continue;
        }
        action(
            &compositor,
            format!(
                "touch-motion gtk-native {slot} {} {}",
                w["x"].as_f64().unwrap() + 80.0,
                w["y"].as_f64().unwrap() + 50.0
            ),
        );
        action(&compositor, format!("touch-up gtk-native {slot}"));
        frame(&compositor);
        let events = terminal_events(&mut client);
        assert!(
            matches!(events.as_slice(), [
                Event::Motion { id: moved, x, y, .. }, Event::Up { id: ended, .. },
            ] if *moved == slot && *ended == slot && (*x, *y) == (80.0, 50.0)),
            "rejected GTK gesture must preserve native contact completion: {events:?}",
        );
    }
    for serial in [original, second] {
        client.gtk_gesture(0, serial, Gesture::DoubleClick);
        state(&compositor, &mut client, id, false, false);
    }
    fresh_gesture(&compositor, &mut client, id);
}

#[test]
fn second_contact_revokes_touch_titlebar_authority() {
    multi_contact_token(&[]);
}

#[test]
fn touch_titlebar_authority_stays_revoked_after_original_contact_lifts() {
    multi_contact_token(&[0]);
}

#[test]
fn touch_titlebar_authority_stays_revoked_after_second_contact_lifts() {
    multi_contact_token(&[1]);
}

#[test]
fn touch_titlebar_authority_stays_revoked_after_original_contact_completes_last() {
    multi_contact_token(&[1, 0]);
}

#[test]
fn touch_titlebar_authority_stays_revoked_after_second_contact_completes_last() {
    multi_contact_token(&[0, 1]);
}

fn claimed_token(resize: bool) {
    let _guard = headless_test_guard();
    let (compositor, mut client, id) = native_fixture();
    let serial = down(&compositor, &mut client, id);
    if resize {
        client.request_resize(0, serial);
    } else {
        client.request_move(0, serial);
    }
    assert_cancel(&mut client);
    action(&compositor, "touch-motion gtk-native 0 160 180");
    frame(&compositor);
    up(&compositor);
    assert!(terminal_events(&mut client).is_empty());
    let claimed = state(&compositor, &mut client, id, false, false);
    assert_eq!(
        claimed[if resize { "content_width" } else { "canvas_x" }],
        if resize { 260 } else { 120 },
    );
    // First GTK request is after takeover finished: cleanup cannot revive it.
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    let after = state(&compositor, &mut client, id, false, false);
    for field in ["canvas_x", "canvas_y", "content_width", "content_height"] {
        assert_eq!(after[field], claimed[field]);
    }
    fresh_gesture(&compositor, &mut client, id);
}

#[test]
fn native_move_claim_stays_consumed_after_lift() {
    claimed_token(false);
}

#[test]
fn native_resize_claim_stays_consumed_after_lift() {
    claimed_token(true);
}
