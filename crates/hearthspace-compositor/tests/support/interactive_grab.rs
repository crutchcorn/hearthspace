//! CSD requests transfer pointer ownership until completion or owner teardown.
use super::interactive::client::{
    InteractiveClient,
    events::{
        assert_buttons, assert_content_click, assert_keyboard_enter, assert_keyboard_unfocused,
        assert_keys, assert_leave, assert_reentered, assert_silent, motion,
    },
};
use super::navigation::{action, keys, query, wait_windows};
use super::*;
use serde_json::Value;
use wayland_client::protocol::wl_pointer::{ButtonState, Event};

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
    let mut client = InteractiveClient::connect(&socket);
    for window in wait_windows(&compositor, |windows| windows.len() == 2) {
        let x = if window["title"] == "Interactive fixture 0" {
            100
        } else {
            600
        };
        action(&compositor, format!("move-window {} {x} 150", window["id"]));
        assert_eq!(window["titlebar_height"], 0, "fixture must use CSD");
    }
    client.take_pointer_events();
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

fn press(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
    number: usize,
) -> ((f64, f64), u32) {
    let point = center(&window(compositor, number));
    motion(compositor, point);
    client.take_pointer_events();
    action(compositor, "pointer-button-down 272");
    let serial = client.press_serial();
    assert_buttons(
        &client.take_pointer_events(),
        &[(272, ButtonState::Pressed)],
    );
    (point, serial)
}

fn owns_pointer_until_release(operation: Operation) {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    let original = geometry(&window(&compositor, 0));
    let untouched = geometry(&window(&compositor, 1));
    let (point, serial) = press(&compositor, &mut client, 0);
    // Toolkit drag recognition arrives after motion; takeover must keep that
    // original baseline while preventing further events reaching the toolkit.
    motion(&compositor, (point.0 + 20.0, point.1 + 8.0));
    let events = client.take_pointer_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Motion { .. }))
    );
    operation.request(&mut client, 0, serial);
    assert_leave(&mut client, 0);
    assert_eq!(
        geometry(&window(&compositor, 0)),
        operation.expected(original, 20, 8)
    );

    for repeated in [Operation::Move, Operation::Resize] {
        repeated.request(&mut client, 0, serial);
        assert_silent(&mut client, "repeated request changed pointer ownership");
    }
    action(&compositor, "pointer-button-down 273");
    assert_silent(&mut client, "secondary press escaped the operation");
    action(&compositor, "pointer-button-up 273");
    assert_silent(&mut client, "secondary release escaped the operation");
    motion(&compositor, (point.0 + 40.0, point.1 + 24.0));
    assert_silent(&mut client, "owned motion reached the client");
    assert_eq!(
        geometry(&window(&compositor, 0)),
        operation.expected(original, 40, 24)
    );
    assert_eq!(geometry(&window(&compositor, 1)), untouched);

    action(&compositor, "pointer-button-up 272");
    let events = client.take_pointer_events();
    assert_reentered(&client, &events, 0);
    assert!(
        !client.configuration(0).1,
        "completion must clear resizing state"
    );
    let completed = geometry(&window(&compositor, 0));
    operation.request(&mut client, 0, serial);
    assert_silent(
        &mut client,
        "completed operation accepted its old press serial",
    );
    assert_content_click(&compositor, &mut client);
    motion(&compositor, (point.0 + 60.0, point.1 + 40.0));
    let events = client.take_pointer_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Motion { .. }))
    );
    assert_eq!(
        geometry(&window(&compositor, 0)),
        completed,
        "released operation resumed"
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn csd_move_owns_pointer_until_release_and_resumes_content_clicks() {
    owns_pointer_until_release(Operation::Move);
}

#[test]
fn csd_resize_owns_pointer_until_release_and_resumes_content_clicks() {
    owns_pointer_until_release(Operation::Resize);
}

#[test]
fn csd_late_requests_while_secondary_button_held_preserve_content_pointer_focus() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    for operation in [Operation::Move, Operation::Resize] {
        let original = geometry(&window(&compositor, 0));
        let (point, serial) = press(&compositor, &mut client, 0);
        action(&compositor, "pointer-button-down 273");
        action(&compositor, "pointer-button-up 272");
        assert_buttons(
            &client.take_pointer_events(),
            &[(273, ButtonState::Pressed), (272, ButtonState::Released)],
        );
        // Smithay still has the original implicit grab because right is held,
        // but its initiating left press is no longer eligible for an operation.
        operation.request(&mut client, 0, serial);
        assert_silent(&mut client, "late request cleared client pointer focus");
        motion(&compositor, (point.0 + 30.0, point.1 + 20.0));
        let events = client.take_pointer_events();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::Motion { .. }))
        );
        assert_eq!(geometry(&window(&compositor, 0)), original);
        action(&compositor, "pointer-button-up 273");
        assert_buttons(
            &client.take_pointer_events(),
            &[(273, ButtonState::Released)],
        );
        assert_content_click(&compositor, &mut client);
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

fn held_secondary_button_drains_before_reentry(operation: Operation) {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    let original = geometry(&window(&compositor, 0));
    let (point, serial) = press(&compositor, &mut client, 0);
    operation.request(&mut client, 0, serial);
    assert_leave(&mut client, 0);
    action(&compositor, "pointer-button-down 273");
    motion(&compositor, (point.0 + 20.0, point.1 + 10.0));
    assert_silent(&mut client, "owned secondary press or motion leaked");
    let completed = operation.expected(original, 20, 10);
    assert_eq!(geometry(&window(&compositor, 0)), completed);
    action(&compositor, "pointer-button-up 272");
    assert_silent(
        &mut client,
        "left release reentered before right was released",
    );
    assert!(
        !client.configuration(0).1,
        "left release must finish resizing"
    );
    for late in [Operation::Move, Operation::Resize] {
        late.request(&mut client, 0, serial);
        assert_silent(&mut client, "late request interrupted release draining");
    }
    motion(&compositor, (point.0 + 40.0, point.1 + 20.0));
    assert_silent(
        &mut client,
        "motion escaped while the secondary button was held",
    );
    assert_eq!(
        geometry(&window(&compositor, 0)),
        completed,
        "left release did not stop geometry"
    );
    action(&compositor, "pointer-button-up 273");
    let events = client.take_pointer_events();
    assert_reentered(&client, &events, 0);
    assert_content_click(&compositor, &mut client);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn csd_move_drains_secondary_button_after_geometry_finishes() {
    held_secondary_button_drains_before_reentry(Operation::Move);
}

#[test]
fn csd_resize_drains_secondary_button_after_geometry_finishes() {
    held_secondary_button_drains_before_reentry(Operation::Resize);
}

#[derive(Clone, Copy, Debug)]
enum OwnerRetirement {
    Unmap,
    Destroy,
    Disconnect,
}

#[rstest::rstest]
#[case::unmapped_move(Operation::Move, OwnerRetirement::Unmap, false)]
#[case::unmapped_resize(Operation::Resize, OwnerRetirement::Unmap, false)]
#[case::unmapped_move_secondary(Operation::Move, OwnerRetirement::Unmap, true)]
#[case::unmapped_resize_secondary(Operation::Resize, OwnerRetirement::Unmap, true)]
#[case::destroyed_move(Operation::Move, OwnerRetirement::Destroy, true)]
#[case::destroyed_resize(Operation::Resize, OwnerRetirement::Destroy, true)]
#[case::disconnected_move(Operation::Move, OwnerRetirement::Disconnect, true)]
#[case::disconnected_resize(Operation::Resize, OwnerRetirement::Disconnect, true)]
fn csd_retired_owner_restores_keyboard_and_pointer(
    #[case] operation: Operation,
    #[case] retirement: OwnerRetirement,
    #[case] secondary: bool,
) {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    let mut survivor = if matches!(retirement, OwnerRetirement::Disconnect) {
        client.unmap(1);
        let socket = compositor
            .runtime_dir
            .path()
            .join(format!("hearthspace-test-{}", std::process::id()));
        let mut survivor = InteractiveClient::connect(&socket);
        survivor.unmap(0);
        wait_windows(&compositor, |windows| windows.len() == 2);
        action(
            &compositor,
            format!("move-window {} 600 150", window(&compositor, 1)["id"]),
        );
        Some(survivor)
    } else {
        None
    };
    let (point, serial) = press(&compositor, &mut client, 0);
    client.take_keyboard_events();
    if let Some(survivor) = survivor.as_mut() {
        survivor.take_keyboard_events();
        survivor.take_pointer_events();
    }
    operation.request(&mut client, 0, serial);
    assert_leave(&mut client, 0);
    if secondary {
        action(&compositor, "pointer-button-down 273");
    }
    motion(&compositor, (point.0 + 20.0, point.1 + 8.0));
    assert_silent(
        &mut client,
        "owned events reached the client before teardown",
    );
    match retirement {
        OwnerRetirement::Unmap => client.unmap(0),
        OwnerRetirement::Destroy => client.destroy(0),
        OwnerRetirement::Disconnect => client = survivor.unwrap(),
    }
    wait_windows(&compositor, |windows| windows.len() == 1);
    let remaining = window(&compositor, 1);
    let original = geometry(&remaining);
    motion(&compositor, center(&remaining));
    keys(&compositor, &[30]);
    assert_keyboard_unfocused(&mut client);
    action(&compositor, "pointer-button-up 272");
    if secondary {
        assert_silent(&mut client, "left release reentered before right drained");
        keys(&compositor, &[30]);
        assert_keyboard_unfocused(&mut client);
        action(&compositor, "pointer-button-up 273");
    }
    let events = client.take_pointer_events();
    assert_reentered(&client, &events, 1);
    // A content click would hide missing keyboard recovery. Observe enter and
    // real key delivery first, without another motion or focus command.
    assert_keyboard_enter(&mut client, 1);
    keys(&compositor, &[30]);
    assert_keys(&mut client, 30);
    assert_content_click(&compositor, &mut client);
    let point = center(&remaining);
    motion(&compositor, (point.0 + 30.0, point.1 + 20.0));
    client.sync();
    assert_eq!(
        geometry(&window(&compositor, 1)),
        original,
        "teardown transferred the operation"
    );
    // A stale operation can allow content clicks yet reject the next CSD drag.
    let (point, serial) = press(&compositor, &mut client, 1);
    operation.request(&mut client, 1, serial);
    assert_leave(&mut client, 1);
    motion(&compositor, (point.0 + 20.0, point.1 + 10.0));
    assert_silent(&mut client, "survivor did not acquire pointer ownership");
    assert_eq!(
        geometry(&window(&compositor, 1)),
        operation.expected(original, 20, 10),
    );
    action(&compositor, "pointer-button-up 272");
    let events = client.take_pointer_events();
    assert_reentered(&client, &events, 1);
    assert!(!client.configuration(1).1);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
