//! Real Wayland geometry, input, sizing, and work-area behavior for four corners.
use super::navigation::{action, query};
use super::shell_panels::{click, connect};
use super::*;
use hearthspace_common::{
    CANVAS_MAP_APP_ID, CONTROL_CENTER_APP_ID, LAUNCHER_APP_ID, NOTIFICATIONS_APP_ID, ShellCorner,
    shell_corner_app_id,
};
use serde_json::Value;

const COLOR: [u8; 4] = [0x66, 0xcc, 0, 0xff];

fn corner_bounds(compositor: &HeadlessCompositor, corner: &str) -> Value {
    query(compositor, "status")["shell_corners"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["corner"] == corner)
        .unwrap()
        .clone()
}

#[test]
fn four_corners_pin_controls_panels_and_maximized_work_area() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let output = query(&compositor, "status")["outputs"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let mut apps_corner = None;
    for (corner, name, width, x, y) in [
        (ShellCorner::Notifications, "notifications", 48, 12, 12),
        (ShellCorner::System, "system", 320, 948, 12),
        (ShellCorner::Canvas, "canvas", 88, 12, 740),
        (ShellCorner::Apps, "apps", 320, 948, 740),
    ] {
        let window = client.add(
            shell_corner_app_id(corner, Some(&output)),
            (width, 48),
            (8, 6),
            COLOR,
        );
        let bounds = corner_bounds(&compositor, name);
        assert_eq!(
            (bounds["x"].as_i64().unwrap(), bounds["y"].as_i64().unwrap()),
            (x, y)
        );
        assert_eq!(bounds["width"], width);
        click(&compositor, x as f64 + 20.0, y as f64 + 20.0);
        assert_eq!(client.presses().last(), Some(&(window, (28.0, 26.0))));
        if corner == ShellCorner::Apps {
            apps_corner = Some(window);
        }
    }
    let events = std::os::unix::net::UnixDatagram::bind(
        hearthspace_common::shell_event_socket_path(&compositor.command_socket_path),
    )
    .unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    action(&compositor, "focus-launcher");
    assert_eq!(
        client.focus(),
        apps_corner,
        "keyboard Apps List activation reserves the Apps corner"
    );
    let mut event = [0; 4096];
    events.recv(&mut event).unwrap();
    // A direct click cancels the pending search without needing a real shell.
    click(&compositor, 640.0, 400.0);
    let corners = query(&compositor, "status")["shell_corners"].clone();
    action(&compositor, "pan-to 9000 -7000");
    assert_eq!(query(&compositor, "status")["shell_corners"], corners);
    for (app_id, kind, expected_x, expected_y) in [
        (NOTIFICATIONS_APP_ID, "notifications", 12, 72),
        (CONTROL_CENTER_APP_ID, "controls", 908, 72),
        (CANVAS_MAP_APP_ID, "map", 12, 568),
        (LAUNCHER_APP_ID, "search", 500, 568),
    ] {
        let panel = client.add(app_id.into(), (360, 160), (8, 6), COLOR);
        let status = query(&compositor, "status");
        let bounds = status["shell_panels"]
            .as_array()
            .unwrap()
            .iter()
            .find(|panel| panel["kind"] == kind)
            .unwrap();
        assert_eq!(bounds["x"], expected_x);
        assert_eq!(bounds["y"], expected_y);
        click(
            &compositor,
            f64::from(expected_x) + 20.0,
            f64::from(expected_y) + 20.0,
        );
        assert_eq!(client.presses().last(), Some(&(panel, (28.0, 26.0))));
        client.destroy(panel);
    }
    let normal = client.add(
        "dev.hearthspace.CornerTest".into(),
        (240, 160),
        (0, 0),
        COLOR,
    );
    let windows = query(&compositor, "windows");
    assert_eq!(windows["windows"].as_array().unwrap().len(), 1);
    let id = windows["windows"][0]["id"].as_u64().unwrap();
    action(&compositor, format!("maximize-window {id} true"));
    client.sync();
    let window = query(&compositor, "windows")["windows"][0].clone();
    assert_eq!(window["y"], 72);
    assert_eq!(window["height"], 656);
    assert_eq!(client.focus(), Some(normal));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn scaled_small_corners_resize_without_overlapping_and_ignore_unknown_outputs() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_options("640x480", None, None, 2);
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    for (corner, name, width, x, y) in [
        (ShellCorner::Notifications, "notifications", 48, 12, 12),
        (ShellCorner::System, "system", 236, 72, 12),
        (ShellCorner::Canvas, "canvas", 88, 12, 180),
        (ShellCorner::Apps, "apps", 196, 112, 180),
    ] {
        let window = client.add(shell_corner_app_id(corner, None), (1000, 48), (0, 0), COLOR);
        let bounds = corner_bounds(&compositor, name);
        assert_eq!(bounds["width"], width);
        assert_eq!(bounds["x"], x);
        assert_eq!(bounds["y"], y);
        click(&compositor, f64::from(x) + 20.0, f64::from(y) + 20.0);
        assert_eq!(client.presses().last(), Some(&(window, (20.0, 20.0))));
        if corner == ShellCorner::Apps {
            client.resize(window, (100, 48));
            let resized = corner_bounds(&compositor, name);
            assert_eq!(resized["width"], 100);
            assert_eq!(resized["x"], 208);
        }
    }
    client.add(
        shell_corner_app_id(ShellCorner::Apps, Some("unplugged")),
        (100, 48),
        (0, 0),
        COLOR,
    );
    assert_eq!(
        query(&compositor, "status")["shell_corners"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let panel = client.add(LAUNCHER_APP_ID.into(), (500, 500), (0, 0), COLOR);
    let bounds = query(&compositor, "status")["shell_panels"][0].clone();
    assert_eq!(bounds["y"], 72);
    assert_eq!(bounds["height"], 96);
    client.destroy(panel);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
