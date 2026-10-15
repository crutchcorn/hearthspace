//! Client move/resize requests must retain their originating pointer grab.
use super::navigation::{action, query, wait_windows};
use super::*;
use serde_json::Value;

#[path = "interactive_client.rs"]
pub(super) mod client;
use client::InteractiveClient;

#[derive(Clone, Copy, Debug)]
enum Operation {
    Move,
    Resize,
}

impl Operation {
    fn request(self, client: &mut InteractiveClient, window: usize, serial: u32) {
        match self {
            Self::Move => client.request_move(window, serial),
            Self::Resize => client.request_resize(window, serial),
        }
    }
}

fn fixture() -> (HeadlessCompositor, InteractiveClient) {
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id(),));
    let client = InteractiveClient::connect(&socket);
    let windows = wait_windows(&compositor, |windows| windows.len() == 2);
    for window in windows {
        let x = if window["title"] == "Interactive fixture 0" {
            100
        } else {
            600
        };
        action(&compositor, format!("move-window {} {x} 150", window["id"]));
        assert_eq!(window["titlebar_height"], 0, "fixture must use CSD");
    }
    (compositor, client)
}

fn windows(compositor: &HeadlessCompositor) -> Vec<Value> {
    let mut windows = query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .clone();
    windows.sort_by_key(|window| window["title"].as_str().unwrap().to_owned());
    assert_eq!(windows.len(), 2);
    windows
}

fn geometry(window: &Value) -> [i64; 4] {
    ["canvas_x", "canvas_y", "content_width", "content_height"]
        .map(|field| window[field].as_i64().unwrap())
}

fn assert_geometry(compositor: &HeadlessCompositor, before: &[Value], reason: &str) {
    let after = windows(compositor);
    assert_eq!(
        after.iter().map(geometry).collect::<Vec<_>>(),
        before.iter().map(geometry).collect::<Vec<_>>(),
        "{reason}",
    );
}

fn motion(compositor: &HeadlessCompositor, point: (f64, f64)) {
    action(
        compositor,
        format!("pointer-motion-abs {} {}", point.0, point.1),
    );
}

fn press(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
    button: u32,
) -> ((f64, f64), u32) {
    let window = &windows(compositor)[0];
    let point = (
        window["x"].as_f64().unwrap() + window["width"].as_f64().unwrap() / 2.0,
        window["y"].as_f64().unwrap() + window["height"].as_f64().unwrap() / 2.0,
    );
    motion(compositor, point);
    action(compositor, format!("pointer-button-down {button}"));
    (point, client.press_serial())
}

#[test]
fn interactive_client_move_resize_follow_pointer_and_stop_after_release_at_zoom() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let (mut compositor, mut client) = fixture();
    for scale in [1.0_f64, 0.8] {
        if scale < 1.0 {
            action(&compositor, "zoom-out");
            let deadline = Instant::now() + Duration::from_secs(5);
            while query(&compositor, "status")["viewport"]["scale"] != scale {
                assert!(Instant::now() < deadline, "canvas zoom did not settle");
                thread::sleep(Duration::from_millis(20));
            }
        }
        for operation in [Operation::Move, Operation::Resize] {
            let before = windows(&compositor);
            let (point, serial) = press(&compositor, &mut client, 272);
            let first = match operation {
                Operation::Move => 0,
                Operation::Resize => 2,
            };
            let expected_at = |dx: f64, dy: f64| {
                let mut expected = geometry(&before[0]);
                expected[first] += (dx / scale).round() as i64;
                expected[first + 1] += (dy / scale).round() as i64;
                expected
            };
            // A toolkit can request the operation only after processing motion
            // past its drag threshold. Catch up from the original press now,
            // without waiting for another motion event or losing that distance.
            motion(&compositor, (point.0 + 20.0, point.1 + 8.0));
            operation.request(&mut client, 0, serial);
            assert_eq!(
                geometry(&windows(&compositor)[0]),
                expected_at(20.0, 8.0),
                "delayed {operation:?} did not catch up at scale {scale}",
            );
            // Repeated or conflicting requests from this same held button must
            // neither restart the baseline nor replace the existing operation.
            for overlapping in [Operation::Move, Operation::Resize] {
                overlapping.request(&mut client, 0, serial);
                assert_eq!(
                    geometry(&windows(&compositor)[0]),
                    expected_at(20.0, 8.0),
                    "{overlapping:?} interrupted active {operation:?} at scale {scale}",
                );
            }
            motion(&compositor, (point.0 + 40.0, point.1 + 24.0));
            client.sync();
            assert_eq!(
                geometry(&windows(&compositor)[0]),
                expected_at(40.0, 24.0),
                "live {operation:?} at scale {scale}",
            );
            action(&compositor, "pointer-button-up 272");
            client.sync();
            let released = windows(&compositor);
            motion(&compositor, (point.0 + 100.0, point.1 + 80.0));
            client.sync();
            assert_geometry(
                &compositor,
                &released,
                "released operation resumed on motion",
            );
        }
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn interactive_requests_reject_wrong_serial_wrong_window_and_released_grabs() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let (mut compositor, mut client) = fixture();
    for operation in [Operation::Move, Operation::Resize] {
        let before = windows(&compositor);
        let (point, serial) = press(&compositor, &mut client, 272);
        operation.request(&mut client, 0, serial.wrapping_add(1));
        motion(&compositor, (point.0 + 20.0, point.1 + 16.0));
        client.sync();
        assert_geometry(&compositor, &before, "wrong serial started an operation");

        // Both windows deliberately belong to the same Wayland connection:
        // matching only the client does not identify the pressed window.
        operation.request(&mut client, 1, serial);
        motion(&compositor, (point.0 + 40.0, point.1 + 32.0));
        client.sync();
        assert_geometry(
            &compositor,
            &before,
            "grab moved another window of the same client",
        );

        action(&compositor, "pointer-button-up 272");
        client.sync();
        operation.request(&mut client, 0, serial);
        motion(&compositor, (point.0 + 60.0, point.1 + 48.0));
        client.sync();
        assert_geometry(
            &compositor,
            &before,
            "late request installed an operation after release",
        );
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn interactive_requests_reject_non_left_button_grabs() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let (mut compositor, mut client) = fixture();
    for operation in [Operation::Move, Operation::Resize] {
        let before = windows(&compositor);
        let (point, serial) = press(&compositor, &mut client, 273);
        operation.request(&mut client, 0, serial);
        motion(&compositor, (point.0 + 40.0, point.1 + 24.0));
        client.sync();
        assert_geometry(
            &compositor,
            &before,
            "right button started a left-button operation",
        );
        action(&compositor, "pointer-button-up 273");
        client.sync();
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn redundant_restore_requests_acknowledge_configures_and_preserve_interactive_resize() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let (mut compositor, mut client) = fixture();
    let original = windows(&compositor);
    let (configured, resizing) = client.configuration(0);
    assert!(!resizing);
    client.request_restore(0);
    assert_eq!(client.configuration(0), (configured + 2, false));
    assert_geometry(&compositor, &original, "redundant restore changed geometry");

    let (point, serial) = press(&compositor, &mut client, 272);
    client.request_resize(0, serial);
    motion(&compositor, (point.0 + 40.0, point.1 + 24.0));
    let (configured, resizing) = client.configuration(0);
    assert!(resizing);
    let held = windows(&compositor);
    client.request_restore(0);
    assert_eq!(
        client.configuration(0),
        (configured + 2, true),
        "restore acknowledgment must retain pending resizing state"
    );
    assert_geometry(&compositor, &held, "restore interrupted an active resize");

    motion(&compositor, (point.0 + 60.0, point.1 + 40.0));
    client.sync();
    let after = windows(&compositor);
    let mut expected = geometry(&original[0]);
    expected[2] += 60;
    expected[3] += 40;
    assert_eq!(geometry(&after[0]), expected);
    action(&compositor, "pointer-button-up 272");
    client.sync();
    assert!(!client.configuration(0).1);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
