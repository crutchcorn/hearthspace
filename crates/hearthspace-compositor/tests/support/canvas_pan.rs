//! Real pointer/key sequences protect canvas navigation and client ownership.
use super::interactive::client::{
    InteractiveClient,
    events::{assert_buttons, assert_keys, assert_silent, motion},
};
use super::navigation::{action, keys, query, wait_windows};
use super::*;
use serde_json::Value;
use wayland_client::protocol::wl_pointer::ButtonState;

#[path = "canvas_pan/desktop.rs"]
mod desktop;
#[path = "canvas_pan/keyboard.rs"]
mod keyboard;
#[path = "canvas_pan/lifecycle.rs"]
mod lifecycle;
#[path = "canvas_pan/map_shortcut.rs"]
mod map_shortcut;
#[path = "canvas_pan/presentation.rs"]
mod presentation;
#[path = "canvas_pan/sources.rs"]
mod sources;

fn fixture(remote: bool) -> HeadlessCompositor {
    let mut compositor =
        HeadlessCompositor::spawn_with_remote_options("1280x800", None, None, 1, remote);
    drop(compositor.wait_for_socket());
    compositor
}

fn socket(compositor: &HeadlessCompositor) -> PathBuf {
    compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()))
}

fn viewport(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "status")["viewport"].clone()
}

fn offset(compositor: &HeadlessCompositor) -> [i64; 2] {
    let view = viewport(compositor);
    [view["x"].as_i64().unwrap(), view["y"].as_i64().unwrap()]
}

fn tool(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "layout-status")["canvas_tool"].clone()
}

fn start(compositor: &HeadlessCompositor, point: (f64, f64)) {
    motion(compositor, point);
    action(compositor, "pointer-button-down 272");
}

fn finish(compositor: &HeadlessCompositor) {
    action(compositor, "pointer-button-up 272");
}

fn clients(compositor: &HeadlessCompositor) -> InteractiveClient {
    let mut client = InteractiveClient::connect(&socket(compositor));
    for window in wait_windows(compositor, |windows| windows.len() == 2) {
        let x = if window["title"] == "Interactive fixture 0" {
            100
        } else {
            800
        };
        action(compositor, format!("move-window {} {x} 100", window["id"]));
    }
    client.take_pointer_events();
    client.take_keyboard_events();
    client
}

#[test]
fn canvas_pan_matches_reference_drag_and_preserves_scale() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    action(&compositor, "pan-to 5760 3600");
    action(&compositor, "canvas-tool pan");
    assert_eq!(tool(&compositor), "pan");
    start(&compositor, (640., 400.));
    motion(&compositor, (176., 368.));
    assert_eq!(offset(&compositor), [6224, 3632]);
    finish(&compositor);
    assert_eq!(viewport(&compositor)["scale"], 1.);
    assert_eq!(viewport(&compositor)["animating"], false);
    assert_eq!(tool(&compositor), "pan");
}

#[rstest::rstest]
#[case::zoomed_out(0.8, [6340, 3640])]
#[case::zoomed_in(2., [5992, 3616])]
fn canvas_pan_tracks_logical_pointer_distance_at_zoom(
    #[case] scale: f64,
    #[case] expected: [i64; 2],
) {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    action(&compositor, format!("zoom-to {scale}"));
    super::canvas_zoom::wait_scale(&compositor, scale);
    action(&compositor, "pan-to 5760 3600");
    action(&compositor, "canvas-tool pan");
    start(&compositor, (640., 400.));
    motion(&compositor, (176., 368.));
    finish(&compositor);
    assert_eq!(offset(&compositor), expected);
    assert_eq!(viewport(&compositor)["scale"], scale);
}

#[test]
fn canvas_pan_preserves_application_geometry_focus_and_keyboard_delivery() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    let before = query(&compositor, "windows")["windows"].clone();
    action(&compositor, "canvas-tool pan");
    start(&compositor, (500., 500.));
    motion(&compositor, (580., 540.));
    keys(&compositor, &[30]);
    assert_keys(&mut client, 30);
    finish(&compositor);
    assert_silent(&mut client, "canvas drag leaked pointer events to an app");
    let after = query(&compositor, "windows")["windows"].clone();
    for original in before.as_array().unwrap() {
        let current = after
            .as_array()
            .unwrap()
            .iter()
            .find(|window| window["id"] == original["id"])
            .unwrap();
        for field in [
            "canvas_x",
            "canvas_y",
            "content_width",
            "content_height",
            "focused",
        ] {
            assert_eq!(current[field], original[field], "pan changed {field}");
        }
    }
    // Ordinary client content still receives a complete click in selected Pan.
    let window = &after[0];
    start(
        &compositor,
        (
            window["x"].as_f64().unwrap() + 40.,
            window["y"].as_f64().unwrap() + 40.,
        ),
    );
    finish(&compositor);
    assert_buttons(
        &client.take_pointer_events(),
        &[(272, ButtonState::Pressed), (272, ButtonState::Released)],
    );
}

#[test]
fn canvas_tools_require_meta_on_desktop_and_select_commands_are_idempotent() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    keys(&compositor, &[35]); // H belongs to the focused application.
    assert_keys(&mut client, 35);
    assert_eq!(tool(&compositor), "select");
    start(&compositor, (500., 500.));
    finish(&compositor); // Empty canvas gives keyboard authority to the desktop.
    client.take_keyboard_events();
    keys(&compositor, &[35]);
    assert_eq!(tool(&compositor), "select");
    keys(&compositor, &[125, 35]);
    assert_eq!(tool(&compositor), "pan");
    keys(&compositor, &[47]);
    keys(&compositor, &[53]);
    assert_eq!(tool(&compositor), "pan");
    assert_eq!(query(&compositor, "canvas-search-status")["active"], false);
    assert!(client.take_keyboard_events().is_empty());
    for selected in ["pan", "select", "group", "search", "layout", "select"] {
        action(&compositor, format!("canvas-tool {selected}"));
        action(&compositor, format!("canvas-tool {selected}"));
        assert_eq!(tool(&compositor), selected);
    }
}
