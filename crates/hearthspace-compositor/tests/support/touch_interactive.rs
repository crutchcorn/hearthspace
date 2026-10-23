//! Frame-accurate touch input across the asynchronous CSD move handoff.
use super::interactive::client::InteractiveClient;
use super::navigation::{action, query, wait_windows};
use super::*;
use serde_json::Value;
use wayland_client::protocol::wl_pointer;

fn fixture() -> (HeadlessCompositor, InteractiveClient) {
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let client = InteractiveClient::connect(&socket);
    for window in wait_windows(&compositor, |w| w.len() == 2) {
        let x = if window["title"] == "Interactive fixture 0" {
            100
        } else {
            900
        };
        action(&compositor, format!("move-window {} {x} 150", window["id"]));
    }
    (compositor, client)
}

fn window(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["title"] == "Interactive fixture 0")
        .unwrap()
        .clone()
}

fn position(compositor: &HeadlessCompositor) -> (i64, i64) {
    let w = window(compositor);
    (
        w["canvas_x"].as_i64().unwrap(),
        w["canvas_y"].as_i64().unwrap(),
    )
}

fn start(compositor: &HeadlessCompositor, client: &mut InteractiveClient) -> (f64, f64) {
    let w = window(compositor);
    let p = (
        w["x"].as_f64().unwrap() + 40.,
        w["y"].as_f64().unwrap() + 30.,
    );
    action(compositor, format!("touch-down finger 0 {} {}", p.0, p.1));
    action(compositor, "touch-frame finger");
    motion(compositor, (p.0 + 16., p.1 + 8.));
    let serial = client.press_serial();
    client.request_move(0, serial);
    p
}

fn motion(compositor: &HeadlessCompositor, p: (f64, f64)) {
    action(compositor, format!("touch-motion finger 0 {} {}", p.0, p.1));
    action(compositor, "touch-frame finger");
}

fn lift(compositor: &HeadlessCompositor) {
    action(compositor, "touch-up finger 0");
    action(compositor, "touch-frame finger");
}

fn responsive(compositor: &HeadlessCompositor, client: &mut InteractiveClient) {
    let w = window(compositor);
    client.take_pointer_events();
    action(
        compositor,
        format!(
            "pointer-motion-abs {} {}",
            w["x"].as_f64().unwrap() + 60.,
            w["y"].as_f64().unwrap() + 60.
        ),
    );
    action(compositor, "pointer-button-down 272");
    action(compositor, "pointer-button-up 272");
    let events = client.take_pointer_events();
    let buttons: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            wl_pointer::Event::Button { state, .. } => Some(*state),
            _ => None,
        })
        .collect();
    assert_eq!(
        buttons,
        [
            wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed),
            wayland_client::WEnum::Value(wl_pointer::ButtonState::Released)
        ],
        "content must receive a fresh complete click: {events:?}"
    );
}

#[test]
fn csd_touch_move_keeps_ownership_when_physical_mouse_clicks() {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture();
    let p = start(&compositor, &mut client);
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    motion(&compositor, (p.0 + 80., p.1 + 40.));
    assert_eq!(
        position(&compositor),
        (180, 190),
        "mouse release must not release the finger"
    );
    lift(&compositor);
    responsive(&compositor, &mut client);
    let p = start(&compositor, &mut client);
    motion(&compositor, (p.0 + 40., p.1 + 20.));
    lift(&compositor);
    assert_eq!(position(&compositor), (220, 210));
}

#[test]
fn csd_touch_move_flushes_final_motion_and_lift_in_one_frame_at_zoom() {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture();
    action(&compositor, "zoom-to 0.8");
    let deadline = Instant::now() + Duration::from_secs(5);
    while query(&compositor, "status")["viewport"]["animating"] != false {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    let before = position(&compositor);
    let p = start(&compositor, &mut client);
    action(
        &compositor,
        format!("touch-motion finger 0 {} {}", p.0 + 80., p.1 + 40.),
    );
    // No frame between final motion and up: libinput can batch these together.
    lift(&compositor);
    assert_eq!(position(&compositor), (before.0 + 100, before.1 + 50));
    responsive(&compositor, &mut client);
}

fn interrupted(cancel: &str) {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture();
    let p = start(&compositor, &mut client);
    motion(&compositor, (p.0 + 40., p.1 + 20.));
    let before = position(&compositor);
    action(&compositor, cancel);
    action(&compositor, "touch-frame finger");
    motion(&compositor, (p.0 + 100., p.1 + 80.));
    lift(&compositor);
    action(&compositor, "touch-up finger 1");
    action(&compositor, "touch-frame finger");
    assert_eq!(
        position(&compositor),
        before,
        "cancelled contact must not resume"
    );
    responsive(&compositor, &mut client);
    let p = start(&compositor, &mut client);
    motion(&compositor, (p.0 + 32., p.1 + 16.));
    lift(&compositor);
    assert_eq!(position(&compositor), (before.0 + 32, before.1 + 16));
}

#[test]
fn csd_touch_cancel_recovers_content_and_next_drag() {
    interrupted("touch-cancel finger");
}

#[test]
fn csd_touch_device_removal_recovers_content_and_next_drag() {
    interrupted("touch-remove finger");
}

#[test]
fn csd_touch_second_contact_recovers_content_and_next_drag() {
    interrupted("touch-down finger 1 350 300");
}

#[test]
fn csd_touch_late_move_after_lift_is_ignored_without_losing_next_click() {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture();
    action(&compositor, "touch-down finger 0 140 180");
    action(&compositor, "touch-frame finger");
    motion(&compositor, (160., 190.));
    let serial = client.press_serial();
    lift(&compositor);
    let before = position(&compositor);
    client.request_move(0, serial);
    action(&compositor, "pointer-motion-abs 300 300");
    assert_eq!(position(&compositor), before);
    responsive(&compositor, &mut client);
}
