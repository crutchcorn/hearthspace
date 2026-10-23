//! Grid snapping must preserve visible window edges across real client commits.
use super::interactive::client::InteractiveClient;
use super::navigation::{action, query, wait_windows};
use super::*;
use serde_json::Value;
use wayland_protocols::xdg::shell::client::xdg_toplevel::ResizeEdge;

fn fixture(chrome: bool, inset: (i32, i32)) -> (HeadlessCompositor, InteractiveClient, u64) {
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut client = if chrome {
        InteractiveClient::connect_server_chrome(&socket)
    } else {
        InteractiveClient::connect_with_inset(&socket, inset)
    };
    let windows = wait_windows(&compositor, |windows| windows.len() == 2);
    let id = windows
        .iter()
        .find(|window| window["title"] == "Interactive fixture 0")
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    client.unmap(1);
    action(&compositor, format!("move-window {id} 100 150"));
    action(&compositor, format!("resize-window {id} 360 240"));
    client.sync();
    (compositor, client, id)
}

fn window(compositor: &HeadlessCompositor, id: u64) -> Value {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["id"] == id)
        .unwrap()
        .clone()
}

fn visible(window: &Value, inset: (i32, i32), chrome: bool) -> [i64; 4] {
    [
        window["canvas_x"].as_i64().unwrap() + i64::from(inset.0),
        window["canvas_y"].as_i64().unwrap() + i64::from(inset.1),
        window["content_width"].as_i64().unwrap(),
        window["content_height"].as_i64().unwrap() + if chrome { 44 } else { 0 },
    ]
}

fn motion(compositor: &HeadlessCompositor, point: (f64, f64)) {
    action(
        compositor,
        format!("pointer-motion-abs {} {}", point.0, point.1),
    );
}

fn center(window: &Value) -> (f64, f64) {
    (
        window["x"].as_f64().unwrap() + window["width"].as_f64().unwrap() / 2.0,
        window["y"].as_f64().unwrap() + window["height"].as_f64().unwrap() / 2.0,
    )
}

fn client_press(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
    id: u64,
) -> ((f64, f64), u32) {
    let point = center(&window(compositor, id));
    motion(compositor, point);
    action(compositor, "pointer-button-down 272");
    (point, client.press_serial())
}

#[test]
fn alt_snaps_ssd_titlebar_and_resize_border_with_live_modifier_and_stable_configures() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture(true, (0, 0));
    let initial = window(&compositor, id);
    assert_eq!(initial["titlebar_height"], 44);
    assert_eq!(visible(&initial, (0, 0), true), [100, 150, 360, 284]);
    let title = (
        initial["x"].as_f64().unwrap() + 40.0,
        initial["y"].as_f64().unwrap() + 22.0,
    );
    motion(&compositor, title);
    action(&compositor, "pointer-button-down 272");
    motion(&compositor, (title.0 + 19.0, title.1 + 11.0));
    assert_eq!(
        visible(&window(&compositor, id), (0, 0), true),
        [119, 161, 360, 284]
    );
    // Modifier changes must update the same stationary pointer sample, without
    // rebasing the gesture on the previously snapped location.
    for (command, expected) in [
        ("key-down 56", [126, 154, 360, 284]),
        ("key-up 56", [119, 161, 360, 284]),
        ("key-down 56", [126, 154, 360, 284]),
    ] {
        action(&compositor, command);
        assert_eq!(
            visible(&window(&compositor, id), (0, 0), true),
            expected,
            "{command}"
        );
    }
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "key-up 56");

    let moved = window(&compositor, id);
    let border = (
        moved["x"].as_f64().unwrap() + moved["width"].as_f64().unwrap() + 2.0,
        moved["y"].as_f64().unwrap() + 120.0,
    );
    motion(&compositor, border);
    action(&compositor, "pointer-button-down 272");
    motion(&compositor, (border.0 + 19.0, border.1));
    client.sync();
    assert_eq!(
        visible(&window(&compositor, id), (0, 0), true),
        [126, 154, 379, 284]
    );
    for (command, width) in [
        ("key-down 56", 392),
        ("key-up 56", 379),
        ("key-down 56", 392),
    ] {
        action(&compositor, command);
        client.sync();
        assert_eq!(
            visible(&window(&compositor, id), (0, 0), true),
            [126, 154, width, 284],
            "right-edge snapping must leave left, top and bottom fixed"
        );
    }
    let configured = client.configuration(0);
    assert!(configured.1);
    for dx in [20.0, 21.0, 22.0, 23.0] {
        motion(&compositor, (border.0 + dx, border.1));
    }
    assert_eq!(
        client.configuration(0),
        configured,
        "one snapped cell must not queue redundant configures"
    );
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "key-up 56");
    client.sync();
    assert_eq!(
        visible(&window(&compositor, id), (0, 0), true),
        [126, 154, 392, 284]
    );
    assert!(!client.configuration(0).1);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn csd_visible_geometry_snaps_in_negative_world_at_zoom_and_keeps_final_opposite_edges() {
    let _guard = headless_test_guard();
    let inset = (13, 9);
    let (mut compositor, mut client, id) = fixture(false, inset);
    action(&compositor, "zoom-out");
    let deadline = Instant::now() + Duration::from_secs(5);
    while query(&compositor, "status")["viewport"]["scale"] != 0.8 {
        assert!(Instant::now() < deadline, "zoom did not settle");
        thread::sleep(Duration::from_millis(20));
    }
    action(&compositor, "pan-to -500 -350");
    action(&compositor, format!("move-window {id} -313 -209"));
    assert_eq!(
        visible(&window(&compositor, id), inset, false),
        [-300, -200, 360, 240]
    );
    let (point, serial) = client_press(&compositor, &mut client, id);
    client.request_move(0, serial);
    motion(&compositor, (point.0 + 24.0, point.1 + 16.0));
    assert_eq!(
        visible(&window(&compositor, id), inset, false),
        [-270, -180, 360, 240]
    );
    for (command, expected) in [
        ("key-down 56", [-266, -182, 360, 240]),
        ("key-up 56", [-270, -180, 360, 240]),
        ("key-down 56", [-266, -182, 360, 240]),
    ] {
        action(&compositor, command);
        assert_eq!(
            visible(&window(&compositor, id), inset, false),
            expected,
            "{command}"
        );
    }
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "key-up 56");

    let (point, serial) = client_press(&compositor, &mut client, id);
    client.request_resize_edge(0, serial, ResizeEdge::TopLeft);
    action(&compositor, "key-down 56");
    motion(&compositor, (point.0 + 17.6, point.1 + 13.6));
    // The final configure is deliberately committed only after release. The
    // old bottom-right (94,58) must survive this asynchronous final buffer.
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "key-up 56");
    client.sync();
    assert_eq!(
        visible(&window(&compositor, id), inset, false),
        [-238, -154, 332, 212]
    );
    assert!(!client.configuration(0).1);
    motion(&compositor, (point.0 + 80.0, point.1 + 80.0));
    client.sync();
    assert_eq!(
        visible(&window(&compositor, id), inset, false),
        [-238, -154, 332, 212],
        "released resize must not resume"
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn snapped_resize_obeys_client_limits_and_uses_bounds_when_no_grid_dot_is_legal() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture(false, (0, 0));
    client.set_size_limits(0, (321, 190), (347, 220));
    let (point, serial) = client_press(&compositor, &mut client, id);
    client.request_resize(0, serial);
    action(&compositor, "key-down 56");
    motion(&compositor, (point.0 - 200.0, point.1 - 100.0));
    client.sync();
    assert_eq!(
        visible(&window(&compositor, id), (0, 0), false),
        [100, 150, 334, 200],
        "nearest legal dots are right=434 and bottom=350, not off-grid minimum sizes"
    );
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "key-up 56");
    client.sync();

    client.set_size_limits(0, (337, 203), (340, 206));
    let (point, serial) = client_press(&compositor, &mut client, id);
    client.request_resize(0, serial);
    action(&compositor, "key-down 56");
    motion(&compositor, (point.0 + 100.0, point.1 + 100.0));
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "key-up 56");
    client.sync();
    assert_eq!(
        visible(&window(&compositor, id), (0, 0), false),
        [100, 150, 340, 206],
        "no legal dot fits either min/max interval; application limits win"
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn explicit_move_replaces_pending_final_resize_anchor_before_client_commit() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture(false, (0, 0));
    let (point, serial) = client_press(&compositor, &mut client, id);
    client.request_resize_edge(0, serial, ResizeEdge::TopLeft);
    action(&compositor, "key-down 56");
    motion(&compositor, (point.0 + 19.0, point.1 + 11.0));
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "key-up 56");

    // The client has not dispatched either resize configure yet. A newer
    // placement must supersede the old bottom-right anchor even though that
    // resize's final buffer arrives after the explicit move.
    action(&compositor, format!("move-window {id} 500 300"));
    client.sync();
    assert_eq!(
        visible(&window(&compositor, id), (0, 0), false),
        [500, 300, 334, 236],
        "late resize commit must keep the newer explicit placement"
    );
    assert!(!client.configuration(0).1);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
