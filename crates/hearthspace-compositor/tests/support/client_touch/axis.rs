//! Native window operations own wheel input until contacts and buttons drain.
use super::*;
use crate::remote_input::client::RemoteClient;
use wayland_client::{
    WEnum,
    protocol::wl_pointer::{Axis, Event as PointerEvent},
};

fn fixture() -> (HeadlessCompositor, InteractiveClient, RemoteClient) {
    let mut compositor =
        HeadlessCompositor::spawn_with_remote_options("1280x800", None, None, 1, true);
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut client = InteractiveClient::connect(&socket);
    for window in wait_windows(&compositor, |windows| windows.len() == 2) {
        let x = if window["title"] == "Interactive fixture 0" {
            100
        } else {
            900
        };
        action(&compositor, format!("move-window {} {x} 150", window["id"]));
    }
    action(&compositor, "zoom-to 1");
    wait_scale(&compositor, 1.0);
    action(&compositor, "pan-to 0 0");
    client.enable_touch();
    let remote = RemoteClient::connect(&socket);
    (compositor, client, remote)
}

fn assert_axis(client: &mut InteractiveClient, captured: bool) {
    let events = client.take_pointer_events();
    if captured {
        assert!(
            events.is_empty(),
            "wheel input escaped native operation: {events:?}"
        );
        return;
    }
    let axes: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            PointerEvent::Axis {
                axis: WEnum::Value(axis),
                value,
                ..
            } => Some((*axis, *value)),
            _ => None,
        })
        .collect();
    assert_eq!(axes.len(), 2, "wheel input did not resume: {events:?}");
    assert!(axes.contains(&(Axis::HorizontalScroll, 11.0)));
    assert!(axes.contains(&(Axis::VerticalScroll, 17.0)));
    assert_buttons(&events, &[]);
}

fn scroll_both_paths(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
    remote: &mut RemoteClient,
    captured: bool,
) {
    action(compositor, "axis 11 17");
    assert_axis(client, captured);
    // Virtual pointer events enter the same InputEvent::PointerAxis arm as
    // physical hardware, including axis source and frame processing.
    remote.axis(11.0, 17.0);
    assert_axis(client, captured);
}

fn native_operation_captures_axis(resize: bool) {
    let _guard = headless_test_guard();
    let (compositor, mut client, mut remote) = fixture();
    action(&compositor, "pointer-motion-abs 940 190");
    let events = client.take_pointer_events();
    assert!(
        events.iter().any(|event| matches!(event,
            PointerEvent::Enter { surface, .. } if surface == client.surface(1)
        )),
        "mouse must hover the other window before native touch: {events:?}"
    );
    scroll_both_paths(&compositor, &mut client, &mut remote, false);

    let point = screen(&compositor, (40.0, 30.0));
    down(&compositor, 0, point);
    frame(&compositor);
    let serial = assert_down(&mut client, 0, (40.0, 30.0));
    // Native application touch alone does not capture unrelated mouse wheels.
    scroll_both_paths(&compositor, &mut client, &mut remote, false);
    if resize {
        client.request_resize(0, serial);
    } else {
        client.request_move(0, serial);
    }
    assert_cancel(&mut client);
    motion(&compositor, 0, (point.0 + 32.0, point.1 + 16.0));
    frame(&compositor);
    action(&compositor, "pointer-motion-abs 960 210");
    scroll_both_paths(&compositor, &mut client, &mut remote, true);

    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-down 273");
    up(&compositor, 0);
    frame(&compositor);
    scroll_both_paths(&compositor, &mut client, &mut remote, true);
    action(&compositor, "pointer-button-up 272");
    scroll_both_paths(&compositor, &mut client, &mut remote, true);
    action(&compositor, "pointer-button-up 273");
    assert_no_pointer_buttons(&mut client);
    // No new motion: draining the final held button restores wheel delivery.
    scroll_both_paths(&compositor, &mut client, &mut remote, false);
    assert_touch_silent(&mut client);

    // With no mouse buttons held, finger lift alone releases axis ownership.
    down(&compositor, 0, screen(&compositor, (40.0, 30.0)));
    frame(&compositor);
    let serial = assert_down(&mut client, 0, (40.0, 30.0));
    if resize {
        client.request_resize(0, serial);
    } else {
        client.request_move(0, serial);
    }
    assert_cancel(&mut client);
    scroll_both_paths(&compositor, &mut client, &mut remote, true);
    up(&compositor, 0);
    frame(&compositor);
    assert_no_pointer_buttons(&mut client);
    scroll_both_paths(&compositor, &mut client, &mut remote, false);
    assert_touch_silent(&mut client);
}

#[test]
fn native_move_captures_axis_until_finger_and_mouse_buttons_drain() {
    native_operation_captures_axis(false);
}

#[test]
fn native_resize_captures_axis_until_finger_and_mouse_buttons_drain() {
    native_operation_captures_axis(true);
}
