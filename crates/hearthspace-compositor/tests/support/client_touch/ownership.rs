//! A native window operation cannot steal an existing compositor mouse hold.
use super::*;
use wayland_client::protocol::wl_pointer::{ButtonState, Event as PointerEvent};
use wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::Mode;

#[derive(Clone, Copy, Debug)]
enum Operation {
    Move,
    Resize,
}

impl Operation {
    fn request(self, client: &mut InteractiveClient, serial: u32) {
        match self {
            Self::Move => client.request_move(0, serial),
            Self::Resize => client.request_resize(0, serial),
        }
    }

    fn geometry(self, mut before: [i64; 4]) -> [i64; 4] {
        let axis = match self {
            Self::Move => 0,
            Self::Resize => 2,
        };
        before[axis] += 32;
        before[axis + 1] += 16;
        before
    }
}

fn geometry(compositor: &HeadlessCompositor) -> [i64; 4] {
    let window = window(compositor);
    ["canvas_x", "canvas_y", "content_width", "content_height"]
        .map(|field| window[field].as_i64().unwrap())
}

fn pointer_motion(compositor: &HeadlessCompositor, point: (f64, f64)) {
    action(
        compositor,
        format!("pointer-motion-abs {} {}", point.0, point.1),
    );
}

fn held_control_rejects_operation(operation: Operation, maximize: bool) {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture((0, 0), 1.0);
    client.enable_touch();
    let mut controls = crate::shell_panels::connect(&compositor);
    const APP_ID: &str = "dev.hearthspace.TouchOwnershipSsd";
    controls.add_decorated(
        APP_ID.into(),
        (320, 200),
        [40, 180, 110, 255],
        Mode::ServerSide,
    );
    let ssd = wait_windows(&compositor, |windows| windows.len() == 3)
        .into_iter()
        .find(|window| window["app_id"] == APP_ID)
        .unwrap();
    assert_eq!(ssd["titlebar_height"], 44);
    action(&compositor, format!("move-window {} 600 450", ssd["id"]));
    let ssd = query(&compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["app_id"] == APP_ID)
        .unwrap()
        .clone();
    let control = (
        ssd["x"].as_f64().unwrap() + ssd["width"].as_f64().unwrap()
            - if maximize { 58.0 } else { 22.0 },
        ssd["y"].as_f64().unwrap() + 22.0,
    );
    pointer_motion(&compositor, control);
    action(&compositor, "pointer-button-down 272");
    assert!(
        controls.presses().is_empty(),
        "SSD consumed the mouse press"
    );
    assert_no_pointer_buttons(&mut client);

    let before = geometry(&compositor);
    let p = screen(&compositor, (40.0, 30.0));
    down(&compositor, 0, p);
    frame(&compositor);
    let serial = assert_down(&mut client, 0, (40.0, 30.0));
    operation.request(&mut client, serial);
    assert_touch_silent(&mut client);
    motion(&compositor, 0, (p.0 + 24.0, p.1 + 12.0));
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [Event::Motion { id: 0, x, y, .. }, Event::Frame]
            if (*x, *y) == (64.0, 42.0)),
        "{operation:?} must preserve client touch while an SSD control owns the mouse: {events:?}",
    );
    assert_eq!(geometry(&compositor), before);

    // Releasing outside cancels the control; its release must reach its owner
    // even though the native finger remains down on a different window.
    pointer_motion(&compositor, screen(&compositor, (60.0, 50.0)));
    action(&compositor, "pointer-button-up 272");
    up(&compositor, 0);
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(matches!(
        events.as_slice(),
        [Event::Up { id: 0, .. }, Event::Frame]
    ));
    assert_no_pointer_buttons(&mut client);
    assert!(controls.closed().is_empty());
    assert_eq!(
        query(&compositor, "windows")["windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|window| window["app_id"] == APP_ID)
            .unwrap()["maximized"],
        false,
    );

    pointer_motion(&compositor, screen(&compositor, (60.0, 50.0)));
    client.take_pointer_events();
    pointer_motion(&compositor, screen(&compositor, (70.0, 60.0)));
    let events = client.take_pointer_events();
    assert!(
        events.iter().any(
            |event| matches!(event, PointerEvent::Motion { surface_x, surface_y, .. }
            if (*surface_x, *surface_y) == (70.0, 60.0))
        ),
        "a stale SSD mouse hold swallowed subsequent motion: {events:?}",
    );
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    assert_buttons(
        &client.take_pointer_events(),
        &[(272, ButtonState::Pressed), (272, ButtonState::Released)],
    );

    // Once both inputs drain, the same native operation must work normally.
    down(&compositor, 0, p);
    frame(&compositor);
    let fresh = assert_down(&mut client, 0, (40.0, 30.0));
    assert_ne!(fresh, serial);
    operation.request(&mut client, fresh);
    assert_cancel(&mut client);
    motion(&compositor, 0, (p.0 + 32.0, p.1 + 16.0));
    up(&compositor, 0);
    frame(&compositor);
    client.sync();
    assert_eq!(geometry(&compositor), operation.geometry(before));
    assert!(!client.configuration(0).1);
    assert_touch_silent(&mut client);
}

#[test]
fn held_ssd_maximize_rejects_native_move_and_preserves_mouse_recovery() {
    held_control_rejects_operation(Operation::Move, true);
}

#[test]
fn held_ssd_close_rejects_native_move_and_preserves_mouse_recovery() {
    held_control_rejects_operation(Operation::Move, false);
}

#[test]
fn held_ssd_maximize_rejects_native_resize_and_preserves_mouse_recovery() {
    held_control_rejects_operation(Operation::Resize, true);
}

#[test]
fn held_ssd_close_rejects_native_resize_and_preserves_mouse_recovery() {
    held_control_rejects_operation(Operation::Resize, false);
}
