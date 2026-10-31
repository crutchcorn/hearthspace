//! Empty-canvas admission must not replace existing desktop or client sequences.
use super::*;
use serde_json::json;
use wayland_client::protocol::wl_pointer;

fn icon(compositor: &HeadlessCompositor) -> crate::canvas_map_client::MapClient {
    let mut client = crate::shell_panels::connect(compositor);
    let id = "file:6d657461";
    client.add(
        hearthspace_common::desktop_item_app_id(id),
        (100, 108),
        (0, 0),
        [40, 180, 100, 255],
    );
    desktop::place(
        compositor,
        id,
        json!({"surface":"canvas", "x":100., "y":100.}),
    );
    action(compositor, "canvas-tool select");
    client
}

fn item(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "desktop-status")["items"][0].clone()
}

#[test]
fn meta_pan_leaves_icons_in_place_then_the_next_unmodified_drag_selects_them() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = icon(&compositor);
    let before = item(&compositor);
    action(&compositor, "key-down 125");
    start(&compositor, (500., 500.));
    motion(&compositor, (540., 540.));
    action(&compositor, "key-up 125");
    finish(&compositor);
    assert_eq!(offset(&compositor), [-40, -40]);
    assert_eq!(item(&compositor), before);
    assert_eq!(tool(&compositor), "select");
    start(&compositor, (40., 40.));
    motion(&compositor, (280., 300.));
    finish(&compositor);
    assert_eq!(offset(&compositor), [-40, -40]);
    assert_eq!(item(&compositor)["selected"], true);
    assert_eq!(item(&compositor)["x"], before["x"]);
    assert_eq!(item(&compositor)["y"], before["y"]);
    assert!(client.presses().is_empty());
    assert!(client.releases().is_empty());
}

#[test]
fn meta_pressed_after_down_keeps_the_existing_desktop_marquee() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let _client = icon(&compositor);
    start(&compositor, (50., 50.));
    action(&compositor, "key-down 125");
    motion(&compositor, (250., 270.));
    assert_eq!(item(&compositor)["selected"], true);
    finish(&compositor);
    action(&compositor, "key-up 125");
    assert_eq!(offset(&compositor), [0, 0]);
    assert_eq!(item(&compositor)["selected"], true);
    assert_eq!(tool(&compositor), "select");
}

#[test]
fn meta_on_an_icon_keeps_selection_and_dragging_but_selected_pan_still_pans_icons() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let _client = icon(&compositor);
    let before = item(&compositor);
    action(&compositor, "key-down 125");
    start(&compositor, (144., 140.));
    motion(&compositor, (184., 180.));
    finish(&compositor);
    action(&compositor, "key-up 125");
    let moved = item(&compositor);
    assert_eq!(offset(&compositor), [0, 0]);
    assert_eq!(moved["selected"], true);
    assert_ne!(moved["x"], before["x"]);
    assert_ne!(moved["y"], before["y"]);
    action(&compositor, "canvas-tool pan");
    let before = item(&compositor);
    let point = (
        before["x"].as_f64().unwrap() + 44.,
        before["y"].as_f64().unwrap() + 40.,
    );
    action(&compositor, "key-down 125");
    start(&compositor, point);
    motion(&compositor, (point.0 + 40., point.1 + 40.));
    finish(&compositor);
    action(&compositor, "key-up 125");
    assert_eq!(offset(&compositor), [-40, -40]);
    assert_eq!(item(&compositor), before);
    assert_eq!(tool(&compositor), "pan");
}

#[test]
fn meta_client_drag_keeps_its_implicit_grab_and_middle_button_delivery() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    action(&compositor, "key-down 125");
    start(&compositor, (140., 140.));
    motion(&compositor, (500., 500.));
    action(&compositor, "pointer-button-down 274");
    motion(&compositor, (550., 550.));
    finish(&compositor);
    action(&compositor, "pointer-button-up 274");
    action(&compositor, "key-up 125");
    assert_eq!(offset(&compositor), [0, 0]);
    let events = client.take_pointer_events();
    assert_buttons(
        &events,
        &[
            (272, ButtonState::Pressed),
            (274, ButtonState::Pressed),
            (272, ButtonState::Released),
            (274, ButtonState::Released),
        ],
    );
    assert!(events.iter().all(|event| !matches!(event,
        wl_pointer::Event::Enter { surface, .. } if surface != client.surface(0))));
    client.take_keyboard_events();
    keys(&compositor, &[30]);
    assert_keys(&mut client, 30);
}

#[test]
fn meta_pressed_during_a_client_window_move_never_adopts_its_drag() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    start(&compositor, (140., 140.));
    let serial = client.press_serial();
    client.request_move(0, serial);
    action(&compositor, "key-down 125");
    motion(&compositor, (550., 550.));
    finish(&compositor);
    action(&compositor, "key-up 125");
    assert_eq!(offset(&compositor), [0, 0]);
    let windows = query(&compositor, "windows");
    let moved = windows["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["title"] == "Interactive fixture 0")
        .unwrap();
    assert_eq!(moved["canvas_x"], 510);
    assert_eq!(moved["canvas_y"], 510);
    assert_eq!(tool(&compositor), "select");
}

#[test]
fn meta_pan_preserves_a_selected_group_and_excludes_its_gather_button() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    action(&compositor, "canvas-tool group");
    keys(&compositor, &[57]);
    keys(&compositor, &[28]);
    let selected = query(&compositor, "group-status")["selected"].clone();
    assert_eq!(selected.as_array().unwrap().len(), 1);
    action(&compositor, "key-down 125");
    start(&compositor, (500., 500.));
    motion(&compositor, (550., 550.));
    finish(&compositor);
    action(&compositor, "key-up 125");
    assert_eq!(offset(&compositor), [-50, -50]);
    assert_eq!(query(&compositor, "group-status")["selected"], selected);
    let bounds = query(&compositor, "group-status")["gather_button"].clone();
    let point = (
        bounds["x"].as_f64().unwrap() + 10.,
        bounds["y"].as_f64().unwrap() + 10.,
    );
    action(&compositor, "key-down 125");
    start(&compositor, point);
    motion(&compositor, (point.0 - 40., point.1 + 40.));
    finish(&compositor);
    action(&compositor, "key-up 125");
    assert_eq!(
        offset(&compositor),
        [-50, -50],
        "Gather admitted a temporary pan"
    );
    assert_buttons(&client.take_pointer_events(), &[]);
}
