//! Native touch serials authorize CSD operations without a pointer press.
use super::*;

#[path = "two_contact_authority.rs"]
mod two_contact;

#[derive(Clone, Copy, Debug)]
enum Operation {
    Move,
    Resize,
}

impl Operation {
    fn request(self, client: &mut InteractiveClient, number: usize, serial: u32) {
        match self {
            Self::Move => client.request_move(number, serial),
            Self::Resize => client.request_resize(number, serial),
        }
    }

    fn geometry(self, mut before: [i64; 4], delta: (i64, i64)) -> [i64; 4] {
        let axis = match self {
            Self::Move => 0,
            Self::Resize => 2,
        };
        before[axis] += delta.0;
        before[axis + 1] += delta.1;
        before
    }
}

fn geometry(window: &Value) -> [i64; 4] {
    ["canvas_x", "canvas_y", "content_width", "content_height"]
        .map(|field| window[field].as_i64().unwrap())
}

fn geometries(compositor: &HeadlessCompositor) -> Vec<[i64; 4]> {
    let mut windows = query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .clone();
    windows.sort_by_key(|window| window["title"].as_str().unwrap().to_owned());
    windows.iter().map(geometry).collect()
}

fn assert_geometry(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
    expected: [i64; 4],
) {
    client.sync();
    assert_eq!(geometry(&window(compositor)), expected);
}

fn native_operation(operation: Operation) {
    let _guard = headless_test_guard();
    let inset = (12, 8);
    let (compositor, mut client) = fixture(inset, 0.5);
    client.enable_touch();
    let before = geometry(&window(&compositor));
    let p = screen(&compositor, (40.0, 30.0));
    down(&compositor, 0, p);
    frame(&compositor);
    let serial = assert_down(&mut client, 0, (52.0, 38.0));
    motion(&compositor, 0, (p.0 + 16.0, p.1 + 8.0));
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [Event::Motion { id: 0, x, y, .. }, Event::Frame]
            if (*x, *y) == (84.0, 54.0)),
        "toolkit must receive motion before requesting {operation:?}: {events:?}",
    );
    operation.request(&mut client, 0, serial);
    assert_cancel(&mut client);
    // A delayed toolkit request catches up from the original native down.
    assert_geometry(
        &compositor,
        &mut client,
        operation.geometry(before, (32, 16)),
    );
    assert_eq!(
        client.configuration(0).1,
        matches!(operation, Operation::Resize)
    );

    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    // These are physical mouse events; native touch alone must not emulate them.
    client.take_pointer_events();
    assert_geometry(
        &compositor,
        &mut client,
        operation.geometry(before, (32, 16)),
    );
    motion(&compositor, 0, (p.0 + 32.0, p.1 + 16.0));
    frame(&compositor);
    assert_geometry(
        &compositor,
        &mut client,
        operation.geometry(before, (64, 32)),
    );

    motion(&compositor, 0, (p.0 + 48.0, p.1 + 24.0));
    // Final motion and lift share a libinput frame; neither may be dropped.
    up(&compositor, 0);
    frame(&compositor);
    let final_geometry = operation.geometry(before, (96, 48));
    assert_geometry(&compositor, &mut client, final_geometry);
    assert!(
        !client.configuration(0).1,
        "resize must finish on native lift"
    );
    assert_touch_silent(&mut client);
    motion(&compositor, 0, (p.0 + 80.0, p.1 + 50.0));
    frame(&compositor);
    assert_geometry(&compositor, &mut client, final_geometry);
    assert_touch_silent(&mut client);
    fresh_tap(&compositor, &mut client, inset);
}

#[test]
fn native_touch_move_at_zoom_preserves_ownership_and_batched_final_motion() {
    native_operation(Operation::Move);
}

#[test]
fn native_touch_resize_at_zoom_preserves_ownership_and_batched_final_motion() {
    native_operation(Operation::Resize);
}

fn rejected_serials(operation: Operation) {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture((0, 0), 0.5);
    client.enable_touch();
    let before = geometries(&compositor);
    let p = screen(&compositor, (40.0, 30.0));
    down(&compositor, 0, p);
    frame(&compositor);
    let serial = assert_down(&mut client, 0, (40.0, 30.0));
    operation.request(&mut client, 0, serial.wrapping_add(100_000));
    motion(&compositor, 0, (p.0 + 8.0, p.1 + 4.0));
    frame(&compositor);
    client.sync();
    assert_eq!(
        geometries(&compositor),
        before,
        "invented touch serial accepted"
    );

    // A matching connection is insufficient: this down belongs to window 0.
    operation.request(&mut client, 1, serial);
    motion(&compositor, 0, (p.0 + 16.0, p.1 + 8.0));
    frame(&compositor);
    client.sync();
    assert_eq!(
        geometries(&compositor),
        before,
        "wrong window accepted touch serial"
    );
    up(&compositor, 0);
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(
            events.as_slice(),
            [
                Event::Motion { id: 0, .. },
                Event::Frame,
                Event::Motion { id: 0, .. },
                Event::Frame,
                Event::Up { id: 0, .. },
                Event::Frame,
            ]
        ),
        "rejected requests must preserve the native contact: {events:?}",
    );

    operation.request(&mut client, 0, serial);
    motion(&compositor, 0, (p.0 + 40.0, p.1 + 20.0));
    frame(&compositor);
    // A late request must not attach a move/resize to the physical mouse either.
    action(&compositor, "pointer-motion-abs 1100 700");
    client.sync();
    assert_eq!(
        geometries(&compositor),
        before,
        "late touch request accepted"
    );
    assert_touch_silent(&mut client);

    down(&compositor, 0, p);
    frame(&compositor);
    let fresh = assert_down(&mut client, 0, (40.0, 30.0));
    assert_ne!(fresh, serial);
    operation.request(&mut client, 0, serial);
    motion(&compositor, 0, (p.0 + 16.0, p.1 + 8.0));
    frame(&compositor);
    client.sync();
    assert_eq!(
        geometries(&compositor),
        before,
        "prior down replayed during a new contact"
    );
    let events = client.take_touch_events();
    assert!(
        matches!(
            events.as_slice(),
            [Event::Motion { id: 0, .. }, Event::Frame]
        ),
        "replay must not cancel the legitimate new contact: {events:?}",
    );

    // The actual current serial still works after every rejected request.
    operation.request(&mut client, 0, fresh);
    assert_cancel(&mut client);
    assert_geometry(
        &compositor,
        &mut client,
        operation.geometry(before[0], (32, 16)),
    );
    up(&compositor, 0);
    frame(&compositor);
    assert_touch_silent(&mut client);

    // Replay the consumed operation serial after another fresh down too.
    let after = geometries(&compositor);
    down(&compositor, 0, screen(&compositor, (40.0, 30.0)));
    frame(&compositor);
    assert_down(&mut client, 0, (40.0, 30.0));
    operation.request(&mut client, 0, fresh);
    motion(&compositor, 0, screen(&compositor, (80.0, 60.0)));
    frame(&compositor);
    client.sync();
    assert_eq!(
        geometries(&compositor),
        after,
        "consumed operation serial replayed"
    );
    let events = client.take_touch_events();
    assert!(
        matches!(
            events.as_slice(),
            [Event::Motion { id: 0, .. }, Event::Frame]
        ),
        "consumed serial must not cancel a new contact: {events:?}",
    );
    up(&compositor, 0);
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(matches!(
        events.as_slice(),
        [Event::Up { id: 0, .. }, Event::Frame]
    ));
    assert_no_pointer_buttons(&mut client);
}

#[test]
fn native_touch_move_rejects_invalid_late_and_replayed_serials() {
    rejected_serials(Operation::Move);
}

#[test]
fn native_touch_resize_rejects_invalid_late_and_replayed_serials() {
    rejected_serials(Operation::Resize);
}
