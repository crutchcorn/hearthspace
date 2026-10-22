//! Real Wayland geometry, input, sizing, and work-area behavior for four corners.
use super::navigation::{action, query};
use super::shell_panels::{click, connect};
use super::*;
use hearthspace_common::{
    CANVAS_MAP_APP_ID, LAUNCHER_APP_ID, NOTIFICATIONS_APP_ID, QUICK_PANEL_APP_ID, ShellCorner,
    shell_corner_app_id,
};
use serde_json::Value;

const COLOR: [u8; 4] = [0x66, 0xcc, 0, 0xff];

#[path = "shell_corners/focused_view.rs"]
mod focused_view;

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
fn stationary_pointer_click_uses_resized_corner_geometry_and_preserves_held_release() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_options("2560x1600", None, None, 2);
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let apps = client.add(
        shell_corner_app_id(ShellCorner::Apps, None),
        (124, 54),
        (0, 0),
        COLOR,
    );
    action(&compositor, "pointer-motion-abs 1232 749");
    client.resize(apps, (86, 54));
    assert_eq!(corner_bounds(&compositor, "apps")["x"], 1170);
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.presses().last(), Some(&(apps, (62.0, 27.0))));

    // A press owns its matching release even if its corner moves meanwhile,
    // and the release uses the new local position under the stationary pointer.
    client.resize(apps, (124, 54));
    action(&compositor, "pointer-button-down 272");
    assert_eq!(client.presses().last(), Some(&(apps, (100.0, 27.0))));
    client.resize(apps, (86, 54));
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.releases().last(), Some(&(apps, (62.0, 27.0))));
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.presses().last(), Some(&(apps, (62.0, 27.0))));
    assert_eq!(client.releases().len(), 3);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn keyboard_cycles_corners_and_escape_restores_apps_without_moving_the_canvas() {
    use super::navigation::keys;

    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let normal = client.add(
        "dev.hearthspace.KeyboardCornerTest".into(),
        (240, 160),
        (0, 0),
        COLOR,
    );
    let mut corners = Vec::new();
    for corner in [
        ShellCorner::Notifications,
        ShellCorner::System,
        ShellCorner::Canvas,
        ShellCorner::Apps,
    ] {
        corners.push(client.add(shell_corner_app_id(corner, None), (48, 54), (0, 0), COLOR));
    }
    action(&compositor, "pan-to 9000 -7000");
    let viewport = query(&compositor, "status")["viewport"].clone();
    for expected in corners.iter().chain(corners.first()) {
        keys(&compositor, &[125, 64]); // Super+F6
        assert_eq!(client.focus(), Some(*expected));
    }
    keys(&compositor, &[125, 42, 64]); // Super+Shift+F6 wraps backwards.
    assert_eq!(client.focus(), corners.last().copied());
    keys(&compositor, &[1]);
    assert_eq!(client.focus(), Some(normal));
    assert!(
        client.closed().is_empty(),
        "Escape never closes persistent shell controls"
    );
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    keys(&compositor, &[125, 42, 64]);
    assert_eq!(
        client.focus(),
        corners.last().copied(),
        "reverse starts at Apps from an app"
    );
    keys(&compositor, &[1]);

    let panel = client.add(NOTIFICATIONS_APP_ID.into(), (360, 160), (0, 0), COLOR);
    keys(&compositor, &[125, 64]);
    assert_eq!(client.focus(), corners.first().copied());
    assert!(
        !client.closed().contains(&panel),
        "corner traversal retains the panel focus group"
    );
    keys(&compositor, &[1]);
    assert!(
        client.closed().contains(&panel),
        "Escape prioritizes closing an open panel"
    );
    client.destroy(panel);
    assert_eq!(client.focus(), Some(normal));
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    assert!(!client.keys().contains(&64), "F6 must not reach clients");
    assert!(
        !client.keys().contains(&1),
        "Escape release must not leak after restoring app focus"
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
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
        (ShellCorner::Notifications, "notifications", 110, 24, 24),
        (ShellCorner::System, "system", 320, 936, 24),
        (ShellCorner::Canvas, "canvas", 198, 24, 722),
        (ShellCorner::Apps, "apps", 320, 936, 722),
    ] {
        let window = client.add(
            shell_corner_app_id(corner, Some(&output)),
            (width, 54),
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
        (NOTIFICATIONS_APP_ID, "notifications", 24, 90),
        (QUICK_PANEL_APP_ID, "controls", 896, 90),
        (CANVAS_MAP_APP_ID, "map", 24, 550),
        (LAUNCHER_APP_ID, "search", 896, 550),
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
    assert_eq!(window["x"], 24);
    assert_eq!(window["y"], 96);
    assert_eq!(window["width"], 1232);
    assert_eq!(window["height"], 608);
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
        (ShellCorner::Notifications, "notifications", 110, 24, 24),
        (ShellCorner::System, "system", 138, 158, 24),
        (ShellCorner::Canvas, "canvas", 198, 24, 162),
        (ShellCorner::Apps, "apps", 50, 246, 162),
    ] {
        let window = client.add(shell_corner_app_id(corner, None), (1000, 54), (0, 0), COLOR);
        let bounds = corner_bounds(&compositor, name);
        assert_eq!(bounds["width"], width);
        assert_eq!(bounds["x"], x);
        assert_eq!(bounds["y"], y);
        click(&compositor, f64::from(x) + 20.0, f64::from(y) + 20.0);
        assert_eq!(client.presses().last(), Some(&(window, (20.0, 20.0))));
        if corner == ShellCorner::Apps {
            client.resize(window, (40, 54));
            let resized = corner_bounds(&compositor, name);
            assert_eq!(resized["width"], 40);
            assert_eq!(resized["x"], 256);
        }
    }
    client.add(
        shell_corner_app_id(ShellCorner::Apps, Some("unplugged")),
        (100, 54),
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
    assert_eq!(bounds["y"], 0);
    assert_eq!(bounds["height"], 150);
    client.resize(panel, (500, 120));
    let bounds = query(&compositor, "status")["shell_panels"][0].clone();
    assert_eq!(bounds["y"], 30);
    assert_eq!(bounds["height"], 120);
    assert_eq!(
        bounds["y"].as_i64().unwrap() + bounds["height"].as_i64().unwrap(),
        corner_bounds(&compositor, "apps")["y"].as_i64().unwrap() - 12,
        "the app list leaves its own corner exposed"
    );
    click(&compositor, 100.0, 100.0);
    assert_eq!(client.presses().last(), Some(&(panel, (76.0, 70.0))));
    client.destroy(panel);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
