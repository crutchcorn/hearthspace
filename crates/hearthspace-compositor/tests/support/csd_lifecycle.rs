//! Other windows' lifecycle events must not steal an active CSD interaction.
use super::interactive::client::InteractiveClient;
use super::navigation::{action, query, wait_windows};
use super::*;
use serde_json::Value;

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

    fn expected(self, mut original: [i64; 4], dx: i64, dy: i64) -> [i64; 4] {
        let axis = match self {
            Self::Move => 0,
            Self::Resize => 2,
        };
        original[axis] += dx;
        original[axis + 1] += dy;
        original
    }
}

fn fixture() -> (HeadlessCompositor, InteractiveClient) {
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let client = InteractiveClient::connect(&socket);
    for window in wait_windows(&compositor, |windows| windows.len() == 2) {
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

fn window(compositor: &HeadlessCompositor, number: usize) -> Value {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["title"] == format!("Interactive fixture {number}"))
        .unwrap()
        .clone()
}

fn geometry(window: &Value) -> [i64; 4] {
    ["canvas_x", "canvas_y", "content_width", "content_height"]
        .map(|field| window[field].as_i64().unwrap())
}

fn center(window: &Value) -> (f64, f64) {
    (
        window["x"].as_f64().unwrap() + window["width"].as_f64().unwrap() / 2.0,
        window["y"].as_f64().unwrap() + window["height"].as_f64().unwrap() / 2.0,
    )
}

fn motion(compositor: &HeadlessCompositor, point: (f64, f64)) {
    action(
        compositor,
        format!("pointer-motion-abs {} {}", point.0, point.1),
    );
}

fn unrelated_wayland_unmap(operation: Operation) {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    let original = window(&compositor, 0);
    let point = center(&original);
    motion(&compositor, point);
    action(&compositor, "pointer-button-down 272");
    let serial = client.press_serial();
    operation.request(&mut client, serial);
    motion(&compositor, (point.0 + 20.0, point.1 + 8.0));
    client.sync();
    assert_eq!(
        geometry(&window(&compositor, 0)),
        operation.expected(geometry(&original), 20, 8),
    );

    // Both surfaces belong to the same client. Retire by window identity,
    // without treating another surface's null-buffer commit as our teardown.
    client.unmap(1);
    wait_windows(&compositor, |windows| windows.len() == 1);
    motion(&compositor, (point.0 + 40.0, point.1 + 24.0));
    client.sync();
    let expected = operation.expected(geometry(&original), 40, 24);
    assert_eq!(geometry(&window(&compositor, 0)), expected);

    action(&compositor, "pointer-button-up 272");
    client.sync();
    motion(&compositor, (point.0 + 100.0, point.1 + 80.0));
    client.sync();
    assert_eq!(geometry(&window(&compositor, 0)), expected);
    action(&compositor, format!("focus-window {}", original["id"]));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn csd_move_survives_unrelated_wayland_null_buffer_unmap() {
    unrelated_wayland_unmap(Operation::Move);
}

#[test]
fn csd_resize_survives_unrelated_wayland_null_buffer_unmap() {
    unrelated_wayland_unmap(Operation::Resize);
}

fn touch_motion(compositor: &HeadlessCompositor, point: (f64, f64)) {
    action(
        compositor,
        format!("touch-motion lifecycle-touch 0 {} {}", point.0, point.1),
    );
    action(compositor, "touch-frame lifecycle-touch");
}

fn unrelated_fullscreen_during_touch(operation: Operation) {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    let original = window(&compositor, 0);
    let point = center(&original);
    action(
        &compositor,
        format!("touch-down lifecycle-touch 0 {} {}", point.0, point.1),
    );
    action(&compositor, "touch-frame lifecycle-touch");
    touch_motion(&compositor, (point.0 + 20.0, point.1 + 8.0));
    let serial = client.press_serial();
    operation.request(&mut client, serial);
    assert_eq!(
        geometry(&window(&compositor, 0)),
        operation.expected(geometry(&original), 20, 8),
        "asynchronous request must catch up to the finger",
    );

    client.request_fullscreen(1);
    assert_eq!(window(&compositor, 1)["fullscreen"], true);
    touch_motion(&compositor, (point.0 + 40.0, point.1 + 24.0));
    client.sync();
    let expected = operation.expected(geometry(&original), 40, 24);
    assert_eq!(
        geometry(&window(&compositor, 0)),
        expected,
        "another window's presentation change released the finger's operation",
    );

    action(&compositor, "touch-up lifecycle-touch 0");
    action(&compositor, "touch-frame lifecycle-touch");
    client.sync();
    motion(&compositor, (point.0 + 100.0, point.1 + 80.0));
    client.sync();
    assert_eq!(geometry(&window(&compositor, 0)), expected);
    action(&compositor, format!("focus-window {}", original["id"]));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn csd_touch_move_survives_unrelated_fullscreen_request() {
    unrelated_fullscreen_during_touch(Operation::Move);
}

#[test]
fn csd_touch_resize_survives_unrelated_fullscreen_request() {
    unrelated_fullscreen_during_touch(Operation::Resize);
}

#[test]
fn fullscreen_request_still_interrupts_an_actual_touch_canvas_pinch() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    action(&compositor, "touch-down lifecycle-pinch 0 900 600");
    action(&compositor, "touch-down lifecycle-pinch 1 1000 600");
    action(&compositor, "touch-down lifecycle-pinch 2 950 700");
    action(&compositor, "touch-frame lifecycle-pinch");
    action(&compositor, "touch-motion lifecycle-pinch 0 880 580");
    action(&compositor, "touch-motion lifecycle-pinch 1 1020 580");
    action(&compositor, "touch-motion lifecycle-pinch 2 950 720");
    action(&compositor, "touch-frame lifecycle-pinch");
    let zoomed = query(&compositor, "status")["viewport"].clone();
    assert_ne!(
        zoomed["scale"], 1.0,
        "fixture must begin a real canvas pinch"
    );

    client.request_fullscreen(1);
    assert_eq!(window(&compositor, 1)["fullscreen"], true);
    action(&compositor, "touch-motion lifecycle-pinch 0 850 550");
    action(&compositor, "touch-motion lifecycle-pinch 1 1050 550");
    action(&compositor, "touch-motion lifecycle-pinch 2 950 750");
    action(&compositor, "touch-frame lifecycle-pinch");
    action(&compositor, "touch-up lifecycle-pinch 0");
    action(&compositor, "touch-up lifecycle-pinch 1");
    action(&compositor, "touch-up lifecycle-pinch 2");
    action(&compositor, "touch-frame lifecycle-pinch");
    let ended = query(&compositor, "status")["viewport"].clone();
    assert_eq!(ended["pinch_active"], false);
    assert_eq!(ended["animating"], false);
    for field in ["scale", "x", "y"] {
        assert_eq!(
            ended[field], zoomed[field],
            "interrupted canvas pinch changed {field}"
        );
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[cfg(feature = "xwayland")]
fn unrelated_x11_retirement(operation: Operation) {
    use smithay::reexports::x11rb::{
        self,
        connection::Connection,
        protocol::xproto::{AtomEnum, ConnectionExt, CreateWindowAux, PropMode, WindowClass},
        wrapper::ConnectionExt as _,
    };

    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    let deadline = Instant::now() + Duration::from_secs(10);
    let display = loop {
        if let Some(display) = query(&compositor, "status")["xwayland_display"].as_str() {
            break display.to_owned();
        }
        assert!(Instant::now() < deadline, "XWayland did not become ready");
        thread::sleep(Duration::from_millis(20));
    };
    let (connection, screen_index) = x11rb::connect(Some(&display)).unwrap();
    let screen = &connection.setup().roots[screen_index];
    let create = |title: &[u8]| {
        let id = connection.generate_id().unwrap();
        connection
            .create_window(
                screen.root_depth,
                id,
                screen.root,
                800,
                500,
                160,
                100,
                0,
                WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &CreateWindowAux::new().background_pixel(0x204080),
            )
            .unwrap();
        connection
            .change_property8(
                PropMode::REPLACE,
                id,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                title,
            )
            .unwrap();
        connection.map_window(id).unwrap();
        id
    };
    let unmapped = create(b"Lifecycle X11 unmap");
    let marker = create(b"Lifecycle X11 marker");
    connection.flush().unwrap();
    for window in wait_windows(&compositor, |windows| windows.len() == 4) {
        if window["client_type"] == "x11" {
            action(&compositor, format!("move-window {} 800 500", window["id"]));
        }
    }
    // A cached title change on the same X connection proves the XWM ran the
    // preceding removal callback. A window-list disappearance alone can race
    // the XWayland surface's earlier null-buffer commit.
    let removal_barrier = |title: &str| {
        connection
            .change_property8(
                PropMode::REPLACE,
                marker,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                title.as_bytes(),
            )
            .unwrap();
        connection.flush().unwrap();
        wait_windows(&compositor, |windows| {
            windows.iter().any(|window| window["title"] == title)
        });
    };
    let original = window(&compositor, 0);
    let point = center(&original);
    motion(&compositor, point);
    action(&compositor, "pointer-button-down 272");
    let serial = client.press_serial();
    operation.request(&mut client, serial);
    motion(&compositor, (point.0 + 20.0, point.1 + 8.0));
    client.sync();
    assert_eq!(
        geometry(&window(&compositor, 0)),
        operation.expected(geometry(&original), 20, 8),
    );

    connection.unmap_window(unmapped).unwrap();
    removal_barrier("Lifecycle X11 unmap processed");
    motion(&compositor, (point.0 + 40.0, point.1 + 24.0));
    client.sync();
    assert_eq!(
        geometry(&window(&compositor, 0)),
        operation.expected(geometry(&original), 40, 24),
        "unmapping an unrelated X11 window cancelled the operation",
    );

    // Destruction after unmap no longer has a managed entry. Its repeated
    // teardown must still preserve the unrelated window's held operation.
    connection.destroy_window(unmapped).unwrap();
    removal_barrier("Lifecycle X11 destroy processed");
    motion(&compositor, (point.0 + 60.0, point.1 + 40.0));
    client.sync();
    let expected = operation.expected(geometry(&original), 60, 40);
    assert_eq!(
        geometry(&window(&compositor, 0)),
        expected,
        "destroying an unrelated X11 window cancelled the operation",
    );
    action(&compositor, "pointer-button-up 272");
    client.sync();
    motion(&compositor, (point.0 + 100.0, point.1 + 80.0));
    client.sync();
    assert_eq!(geometry(&window(&compositor, 0)), expected);
    action(&compositor, format!("focus-window {}", original["id"]));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[cfg(feature = "xwayland")]
#[test]
fn csd_move_survives_unrelated_x11_unmap_and_destroy() {
    unrelated_x11_retirement(Operation::Move);
}

#[cfg(feature = "xwayland")]
#[test]
fn csd_resize_survives_unrelated_x11_unmap_and_destroy() {
    unrelated_x11_retirement(Operation::Resize);
}
