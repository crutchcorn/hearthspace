//! Server-titlebar restoration against a real XWayland client.
use super::*;
use serde_json::Value;
use smithay::reexports::x11rb::{
    protocol::xproto::{AtomEnum, ConnectionExt, PropMode},
    wrapper::ConnectionExt as _,
};

fn window(compositor: &HeadlessCompositor, id: u64) -> Value {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["id"] == id)
        .unwrap()
        .clone()
}

fn server_chrome(compositor: &HeadlessCompositor, client: &mut Client) {
    let motif = client
        .connection
        .intern_atom(false, b"_MOTIF_WM_HINTS")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    client
        .connection
        .change_property32(
            PropMode::REPLACE,
            client.window,
            motif,
            motif,
            &[2, 0, 1, 0, 0],
        )
        .unwrap();
    client.barrier(compositor);
    let window = window(compositor, client.id);
    assert_eq!(window["titlebar_height"], 44);
    client.clear_events();
}

fn click_maximize(compositor: &HeadlessCompositor, window: &Value) {
    let scale = window["titlebar_height"].as_f64().unwrap() / 44.0;
    let x = window["x"].as_f64().unwrap() + window["width"].as_f64().unwrap() - 58.0 * scale;
    let y = window["y"].as_f64().unwrap() + 22.0 * scale;
    action(compositor, format!("pointer-motion-abs {x} {y}"));
    action(compositor, "pointer-button-down 272");
    action(compositor, "pointer-button-up 272");
}

fn wait_size(compositor: &HeadlessCompositor, client: &Client, size: [u16; 2]) -> Value {
    let window = wait_windows(compositor, |windows| {
        windows.iter().any(|window| {
            window["id"] == client.id
                && window["content_width"] == size[0]
                && window["content_height"] == size[1]
        })
    })
    .into_iter()
    .find(|window| window["id"] == client.id)
    .unwrap();
    let geometry = client
        .connection
        .get_geometry(client.window)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!([geometry.width, geometry.height], size);
    window
}

fn assert_x11_maximized(client: &Client, maximized: bool) {
    let atom = |name: &[u8]| {
        client
            .connection
            .intern_atom(false, name)
            .unwrap()
            .reply()
            .unwrap()
            .atom
    };
    let property = client
        .connection
        .get_property(
            false,
            client.window,
            atom(b"_NET_WM_STATE"),
            AtomEnum::ATOM,
            0,
            u32::MAX,
        )
        .unwrap()
        .reply()
        .unwrap();
    let states: Vec<_> = property.value32().unwrap().collect();
    assert_eq!(
        states.contains(&atom(b"_NET_WM_STATE_MAXIMIZED_HORZ")),
        maximized
    );
    assert_eq!(
        states.contains(&atom(b"_NET_WM_STATE_MAXIMIZED_VERT")),
        maximized
    );
    assert!(!states.contains(&atom(b"_NET_WM_STATE_FULLSCREEN")));
}

#[test]
fn x11_ssd_maximize_button_restores_floating_geometry_and_drag() {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture();
    server_chrome(&compositor, &mut client);
    action(&compositor, "zoom-to 0.5");
    crate::canvas_zoom::wait_scale(&compositor, 0.5);
    let original = window(&compositor, client.id);
    assert_eq!(original["titlebar_height"], 22);

    click_maximize(&compositor, &original);
    client.barrier(&compositor);
    let focused = wait_size(&compositor, &client, [1280, 756]);
    assert_eq!(focused["maximized"], true);
    assert_eq!(focused["focused_view"], true);
    assert_eq!(focused["titlebar_height"], 44);
    assert_x11_maximized(&client, true);

    click_maximize(&compositor, &focused);
    client.barrier(&compositor);
    // Negative control: the old button exits focused view but leaves this flag
    // set, selecting bounded maximization and rejecting the next titlebar drag.
    let state = window(&compositor, client.id);
    assert_eq!(
        state["maximized"], false,
        "restore button left window maximized"
    );
    assert_eq!(state["focused_view"], false);
    assert_eq!(state["fullscreen"], false);
    assert_x11_maximized(&client, false);
    let restored = wait_size(&compositor, &client, [320, 240]);
    for field in ["canvas_x", "canvas_y", "content_width", "content_height"] {
        assert_eq!(restored[field], original[field], "restore {field}");
    }

    let title = (
        restored["x"].as_i64().unwrap() + 20,
        restored["y"].as_i64().unwrap() + 11,
    );
    action(
        &compositor,
        format!("pointer-motion-abs {} {}", title.0, title.1),
    );
    action(&compositor, "pointer-button-down 272");
    action(
        &compositor,
        format!("pointer-motion-abs {} {}", title.0 + 40, title.1 + 20),
    );
    action(&compositor, "pointer-button-up 272");
    client.barrier(&compositor);
    let dragged = window(&compositor, client.id);
    assert_eq!(
        dragged["canvas_x"].as_i64().unwrap(),
        original["canvas_x"].as_i64().unwrap() + 80
    );
    assert_eq!(
        dragged["canvas_y"].as_i64().unwrap(),
        original["canvas_y"].as_i64().unwrap() + 40
    );
    assert_eq!(dragged["maximized"], false);
    wait_size(&compositor, &client, [320, 240]);
    assert!(
        client.presses.is_empty(),
        "server chrome press reached X11 content"
    );
    assert!(
        client.releases.is_empty(),
        "server chrome release reached X11 content"
    );
}
