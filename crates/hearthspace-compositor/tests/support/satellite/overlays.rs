//! Real X11 metadata crosses Satellite without any application-name matching.
use super::*;
use serde_json::Value;

#[path = "overlays/fixture.rs"]
mod fixture;
use fixture::{overlay, overlay_at};

#[track_caller]
fn overlays(c: &HeadlessCompositor, count: usize) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let s = query(c, "status");
        let rows = s["screen_overlays"]
            .as_array()
            .expect("overlay status")
            .clone();
        if rows.len() == count {
            return rows;
        }
        if Instant::now() >= deadline {
            eprintln!("private X window census: {}", fixture::diagnostics(&s));
            panic!("expected {count} overlays: {s}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn generic_overlays_keep_screen_geometry_through_pan_zoom_and_deliver_clicks() {
    let _guard = headless_test_guard();
    let (c, display) = fixture();
    let base = WindowClient::new(&c, &display, "base", 0x202040);
    action(&c, format!("focus-window {}", base.id));
    let (connection, window) = overlay(&c, &display, true, true);
    let rows = overlays(&c, 1);
    let rect = &rows[0];
    assert_eq!(rect["x"], 1056);
    assert_eq!(rect["y"], 696);
    assert_eq!(
        query(&c, "status")["windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["id"] == base.id)
            .unwrap()["focused"],
        true
    );
    action(&c, "pan-to 200 150");
    action(&c, "zoom-to 0.8");
    super::super::canvas_zoom::wait_scale(&c, 0.8);
    assert_eq!(overlays(&c, 1), rows);
    let x = rect["x"].as_i64().unwrap() + 20;
    let y = rect["y"].as_i64().unwrap() + 20;
    action(&c, format!("pointer-motion-abs {x} {y}"));
    action(&c, "pointer-button-down 272");
    action(&c, "pointer-button-up 272");
    expect_click(&connection, window, (20, 20));
    let status = query(&c, "status");
    let focused = status["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == base.id)
        .unwrap();
    assert_eq!(
        focused["focused"], true,
        "a toast click must preserve the application's keyboard focus"
    );
    connection.destroy_window(window).unwrap();
    connection.flush().unwrap();
    overlays(&c, 0);
}
#[test]
fn generic_overlays_stack_and_honor_explicit_notification_type() {
    let _guard = headless_test_guard();
    let (c, display) = fixture();
    let (_a, _) = overlay(&c, &display, true, true);
    let first = overlays(&c, 1);
    let (_b, _) = overlay(&c, &display, false, false);
    let rows = overlays(&c, 2);
    let a = &rows[0];
    let b = &rows[1];
    assert_eq!(a, &first[0]);
    assert_eq!(a["x"], b["x"]);
    assert_eq!(a["y"].as_i64().unwrap() - b["y"].as_i64().unwrap(), 104);
}
#[test]
fn generic_overlays_do_not_adopt_utility_windows_missing_overlay_hints() {
    let _guard = headless_test_guard();
    let (c, display) = fixture();
    let (_connection, _window) = overlay(&c, &display, true, false);
    // Mapping an ordinary window after the candidate establishes processing order.
    let _base = WindowClient::new(&c, &display, "sentinel", 0x202040);
    assert!(
        query(&c, "status")["screen_overlays"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn generic_overlays_leave_clearance_for_the_live_frame_corner() {
    use crate::canvas_map_client::MapClient;
    use hearthspace_common::{ShellCorner, shell_corner_app_id};
    let _guard = headless_test_guard();
    let (c, display) = fixture();
    let socket = c
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut frame = MapClient::connect(&socket);
    frame.add(
        shell_corner_app_id(ShellCorner::Apps, Some("hearthspace-0")),
        (198, 64),
        (0, 0),
        [60, 80, 100, 255],
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let corner = loop {
        let status = query(&c, "status");
        if let Some(row) = status["shell_corners"].as_array().unwrap().first() {
            break row.clone();
        }
        assert!(Instant::now() < deadline, "frame did not map: {status}");
        frame.sync();
        thread::sleep(Duration::from_millis(10));
    };
    let (_connection, _window) = overlay(&c, &display, true, true);
    let rect = overlays(&c, 1).remove(0);
    let r = |v: &Value| -> (i64, i64, i64, i64) {
        (
            v["x"].as_i64().unwrap(),
            v["y"].as_i64().unwrap(),
            v["width"].as_i64().unwrap(),
            v["height"].as_i64().unwrap(),
        )
    };
    let (x, y, w, h) = r(&rect);
    let (fx, fy, fw, fh) = r(&corner);
    assert!(
        x + w + 24 <= fx || fx + fw + 24 <= x || y + h + 24 <= fy || fy + fh + 24 <= y,
        "overlay {rect} overlaps frame safe area {corner}"
    );
}

fn expect_click(connection: &RustConnection, window: Window, local: (i16, i16)) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut phases = Vec::new();
    while phases.len() < 2 {
        if let Some(event) = connection.poll_for_event().unwrap() {
            match event {
                Event::ButtonPress(e) => {
                    assert_eq!((e.event, e.event_x, e.event_y), (window, local.0, local.1));
                    phases.push("press");
                }
                Event::ButtonRelease(e) => {
                    assert_eq!((e.event, e.event_x, e.event_y), (window, local.0, local.1));
                    phases.push("release");
                }
                _ => {}
            }
        } else {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            Instant::now() < deadline,
            "missing click phases: {phases:?}"
        );
    }
    assert_eq!(phases, ["press", "release"]);
}

#[test]
fn generic_overlay_outside_x_root_still_receives_exact_local_click() {
    let _guard = headless_test_guard();
    let (c, display) = super::fixture();
    // Some clients request their old physical bottom edge after a scale change.
    let (connection, window) = overlay_at(&c, &display, true, true, (1100, 840));
    let rect = overlays(&c, 1).remove(0);
    let x = rect["x"].as_i64().unwrap() + 20;
    let y = rect["y"].as_i64().unwrap() + 20;
    action(&c, format!("pointer-motion-abs {x} {y}"));
    action(&c, "pointer-button-down 272");
    action(&c, "pointer-button-up 272");
    expect_click(&connection, window, (20, 20));
}

#[test]
fn generic_overlay_uses_logical_screen_space_on_hidpi_output() {
    let _guard = headless_test_guard();
    let mut c = HeadlessCompositor::spawn_with_options("2560x1600", None, None, 2);
    drop(c.wait_for_socket());
    let deadline = Instant::now() + Duration::from_secs(10);
    let display = loop {
        if let Some(display) = query(&c, "status")["xwayland_display"].as_str() {
            break display.to_owned();
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    };
    let (probe, screen) = x11rb::connect(Some(&display)).unwrap();
    let root = &probe.setup().roots[screen];
    let root_geometry = probe.get_geometry(root.root).unwrap().reply().unwrap();
    let position = (
        (root_geometry.width * 3 / 4) as i16,
        (root_geometry.height * 3 / 4) as i16,
    );
    let (connection, window) = overlay_at(&c, &display, true, true, position);
    let rect = overlays(&c, 1).remove(0);
    assert_eq!(
        rect["x"].as_i64().unwrap() + rect["width"].as_i64().unwrap(),
        1256
    );
    assert_eq!(
        rect["y"].as_i64().unwrap() + rect["height"].as_i64().unwrap(),
        776
    );
    let x = rect["x"].as_i64().unwrap() + 20;
    let y = rect["y"].as_i64().unwrap() + 20;
    action(&c, format!("pointer-motion-abs {x} {y}"));
    action(&c, "pointer-button-down 272");
    action(&c, "pointer-button-up 272");
    expect_click(&connection, window, (40, 40));
}

#[path = "overlays/lifecycle.rs"]
mod lifecycle;
