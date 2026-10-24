//! Explicit GTK titlebar intent crosses the real Wayland socket boundary.
use super::{
    interactive::client::{
        InteractiveClient,
        events::{assert_content_click, assert_reentered},
        gtk::Gesture,
    },
    navigation::{action, keys, query, wait_windows},
    session_lock::lock_client::LockClient,
    *,
};
use serde_json::Value;

#[path = "gtk_titlebar/authority.rs"]
mod authority;
#[path = "gtk_titlebar/configure.rs"]
mod configure;
#[path = "gtk_titlebar/restore.rs"]
mod restore;
#[path = "gtk_titlebar/touch.rs"]
mod touch;

fn socket(compositor: &HeadlessCompositor) -> PathBuf {
    compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()))
}

fn fixture() -> (HeadlessCompositor, InteractiveClient, u64) {
    fixture_with_remote(false)
}

fn fixture_with_remote(remote: bool) -> (HeadlessCompositor, InteractiveClient, u64) {
    let mut compositor =
        HeadlessCompositor::spawn_with_remote_options("1280x800", None, None, 1, remote);
    drop(compositor.wait_for_socket());
    let mut client = InteractiveClient::connect(&socket(&compositor));
    let windows = wait_windows(&compositor, |windows| windows.len() == 2);
    let mut id = 0;
    for window in windows {
        let first = window["title"] == "Interactive fixture 0";
        let x = if first { 100 } else { 700 };
        action(&compositor, format!("move-window {} {x} 150", window["id"]));
        assert_eq!(window["titlebar_height"], 0, "fixture must use CSD");
        if first {
            id = window["id"].as_u64().unwrap();
        }
    }
    action(&compositor, format!("focus-window {id}"));
    // Exercise advisory requests on the unfocused sibling: GTK activation hints
    // cannot take focus, even though both windows share the same connection.
    client.gtk_baseline_requests(1);
    assert_eq!(window(&compositor, id)["focused"], true);
    assert_eq!(client.gtk_edges(0), [1, 2, 3, 4]);
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

fn press(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
    id: u64,
    button: u32,
) -> u32 {
    let window = window(compositor, id);
    action(
        compositor,
        format!(
            "pointer-motion-abs {} {}",
            window["x"].as_f64().unwrap() + 40.0,
            window["y"].as_f64().unwrap() + 20.0
        ),
    );
    action(compositor, format!("pointer-button-down {button}"));
    client.press_serial()
}

fn release(compositor: &HeadlessCompositor, client: &mut InteractiveClient, button: u32) {
    let before = client.released_buttons().len();
    action(compositor, format!("pointer-button-up {button}"));
    assert_eq!(
        &client.released_buttons()[before..],
        &[button],
        "GTK request lost or duplicated its release"
    );
}

fn state(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
    id: u64,
    max: bool,
    focused: bool,
) -> Value {
    client.sync();
    let window = window(compositor, id);
    assert_eq!(window["maximized"], max);
    assert_eq!(window["focused_view"], focused);
    assert_eq!(window["fullscreen"], false);
    window
}

fn double_click(compositor: &HeadlessCompositor, client: &mut InteractiveClient, id: u64) {
    let serial = press(compositor, client, id, 272);
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    // The accepted gesture must retain the application's actual button grab.
    assert!(
        compositor
            .send_text_command("focus-next-window")
            .starts_with("err window action unavailable")
    );
    release(compositor, client, 272);
}

#[test]
fn gtk_titlebar_canvas_restore_and_focused_exit_keep_client_input_and_resize_edges() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture();
    let original = window(&compositor, id);
    double_click(&compositor, &mut client, id);
    let bounded = state(&compositor, &mut client, id, true, false);
    for (field, value) in ["x", "y", "width", "height"]
        .into_iter()
        .zip([24, 96, 1232, 608])
    {
        assert_eq!(bounded[field], value);
    }
    assert!(client.gtk_edges(0).is_empty());
    // GTK4 can repeat these requests from a subsequent layout/present cycle.
    client.gtk_request_maximized(0, true);
    state(&compositor, &mut client, id, true, false);
    double_click(&compositor, &mut client, id);
    let restored = state(&compositor, &mut client, id, false, false);
    assert_eq!(client.gtk_edges(0), [1, 2, 3, 4]);
    for field in ["canvas_x", "canvas_y", "content_width", "content_height"] {
        assert_eq!(restored[field], original[field], "restored {field}");
    }
    // Generic maximize retains CSD and enters focused view; explicit GTK
    // titlebar intent leaves focused view through bounded canvas maximization.
    client.gtk_request_maximized(0, true);
    let focused = state(&compositor, &mut client, id, true, true);
    assert_eq!(focused["width"], 1280);
    assert_eq!(focused["height"], 800);
    assert_eq!(focused["titlebar_height"], 0);
    client.gtk_request_maximized(0, true);
    state(&compositor, &mut client, id, true, true);
    double_click(&compositor, &mut client, id);
    state(&compositor, &mut client, id, true, false);
    client.gtk_request_maximized(0, false);
    state(&compositor, &mut client, id, false, false);
    client.set_size_limits(0, (240, 160), (240, 160));
    assert!(client.gtk_edges(0).is_empty());
    client.set_size_limits(0, (0, 0), (0, 0));
    assert_eq!(client.gtk_edges(0), [1, 2, 3, 4]);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn gtk_titlebar_validates_serial_surface_button_age_and_replay_across_wrappers() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture();
    let serial = press(&compositor, &mut client, id, 272);
    client.gtk_gesture(0, serial.wrapping_add(1), Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);
    client.gtk_gesture(1, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, true, false);
    client.gtk_recreate_surface(0);
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, true, false);
    release(&compositor, &mut client, 272);
    client.gtk_request_maximized(0, false);

    let serial = press(&compositor, &mut client, id, 273);
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    client.gtk_gesture(0, serial, Gesture::RightClick);
    state(&compositor, &mut client, id, false, false);
    release(&compositor, &mut client, 273);
    let serial = press(&compositor, &mut client, id, 274);
    client.gtk_gesture(0, serial, Gesture::MiddleClick);
    state(&compositor, &mut client, id, false, false);
    release(&compositor, &mut client, 274);

    let old = press(&compositor, &mut client, id, 272);
    release(&compositor, &mut client, 272);
    let fresh = press(&compositor, &mut client, id, 272);
    client.gtk_gesture(0, old, Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);
    // Request handling can lag behind a complete physical click or emulated tap.
    release(&compositor, &mut client, 272);
    client.gtk_gesture(0, fresh, Gesture::DoubleClick);
    state(&compositor, &mut client, id, true, false);
    client.gtk_gesture(0, fresh, Gesture::DoubleClick);
    state(&compositor, &mut client, id, true, false);
    client.gtk_request_maximized(0, false);

    let expired = press(&compositor, &mut client, id, 272);
    release(&compositor, &mut client, 272);
    thread::sleep(Duration::from_millis(1100));
    client.gtk_gesture(0, expired, Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn gtk_titlebar_cannot_interrupt_client_move_or_replay_through_switcher_and_lock() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture();
    let serial = press(&compositor, &mut client, id, 272);
    client.request_move(0, serial);
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    let before = state(&compositor, &mut client, id, false, false);
    action(&compositor, "pointer-motion-rel 20 10");
    client.sync();
    let moved = window(&compositor, id);
    assert_eq!(
        moved["canvas_x"].as_i64().unwrap(),
        before["canvas_x"].as_i64().unwrap() + 20
    );
    // The compositor owns this release after move takeover. GTK receives
    // pointer re-entry, then a complete fresh click without another motion.
    client.take_pointer_events();
    let releases = client.released_buttons().len();
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.released_buttons().len(), releases);
    let events = client.take_pointer_events();
    assert_reentered(&client, &events, 0);
    assert_content_click(&compositor, &mut client);

    let serial = press(&compositor, &mut client, id, 272);
    release(&compositor, &mut client, 272);
    action(&compositor, "key-down 56");
    keys(&compositor, &[15]);
    assert_eq!(
        query(&compositor, "status")["window_switcher"]["active"],
        true
    );
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);
    keys(&compositor, &[1]);
    action(&compositor, "key-up 56");
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);

    action(&compositor, format!("focus-window {id}"));
    let serial = press(&compositor, &mut client, id, 272);
    let mut locker = LockClient::connect(&socket(&compositor));
    locker.wait_locked();
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    client.gtk_baseline_requests(0);
    assert_eq!(query(&compositor, "status")["locked"], true);
    locker.unlock();
    wait_windows(&compositor, |windows| {
        windows
            .iter()
            .any(|window| window["id"] == id && window["focused"] == true)
    });
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);
    double_click(&compositor, &mut client, id);
    state(&compositor, &mut client, id, true, false);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn gtk_titlebar_unknown_gesture_is_a_client_protocol_error() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture();
    let serial = press(&compositor, &mut client, id, 272);
    client.gtk_invalid_gesture_disconnects(0, serial);
    wait_windows(&compositor, |windows| windows.is_empty());
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
