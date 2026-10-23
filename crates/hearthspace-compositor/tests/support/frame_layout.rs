//! Frame attachments use real clients while preserving their canonical canvas homes.
use super::navigation::{action, keys, query};
use super::shell_panels::connect;
use super::*;
use serde_json::{Value, json};
#[path = "frame_layout/desktop_layers.rs"]
mod desktop_layers;
#[path = "frame_layout/drag_layer.rs"]
mod drag_layer;
#[path = "frame_layout/geometry.rs"]
mod geometry;
#[path = "frame_layout/group_members.rs"]
mod group_members;
#[path = "frame_layout/guards.rs"]
mod guards;
#[path = "frame_layout/input.rs"]
mod input;
#[path = "frame_layout/member_hit.rs"]
mod member_hit;
#[path = "frame_layout/pending_click.rs"]
mod pending_click;
#[path = "frame_layout/pointer_grab.rs"]
mod pointer_grab;
#[path = "frame_layout/popup_epoch.rs"]
mod popup_epoch;
#[path = "frame_layout/popup_outside.rs"]
mod popup_outside;
#[path = "frame_layout/titlebar.rs"]
mod titlebar;
#[path = "frame_layout/turn.rs"]
mod turn;
#[path = "frame_layout/work_area.rs"]
mod work_area;

const COLOR: [u8; 4] = [44, 110, 230, 255];
fn layout(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "layout-status")
}
fn command(compositor: &HeadlessCompositor, action_name: &str, target: Value, placement: Value) {
    let status = layout(compositor);
    action(
        compositor,
        format!(
            "layout-command {}",
            json!({"session":status["session"],"output":status["output"],"action":action_name,"target":target,"placement":placement})
        ),
    );
}
fn window(compositor: &HeadlessCompositor, app: &str) -> Value {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["app_id"] == app)
        .unwrap()
        .clone()
}
fn pin(compositor: &HeadlessCompositor, id: u64) {
    action(compositor, "layout-tool");
    command(
        compositor,
        "place",
        json!({"kind":"window","id":id.to_string()}),
        json!({"surface":"frame","edge":"right","position":0.5}),
    );
    command(compositor, "done", Value::Null, Value::Null);
}

#[test]
fn frame_layout_projects_live_windows_without_rewriting_canvas_and_reveals_without_panning() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut client = connect(&compositor);
    let number = client.add("frame-layout-window".into(), (500, 320), (0, 0), COLOR);
    let original = window(&compositor, "frame-layout-window");
    let id = original["id"].as_u64().unwrap();
    action(&compositor, format!("move-window {id} 2400 -1600"));
    let before = window(&compositor, "frame-layout-window");
    let viewport = query(&compositor, "status")["viewport"].clone();
    pin(&compositor, id);
    let pinned = window(&compositor, "frame-layout-window");
    assert_eq!(pinned["canvas_x"], before["canvas_x"]);
    assert_eq!(pinned["canvas_y"], before["canvas_y"]);
    assert_eq!(pinned["content_width"], 500);
    assert_eq!(pinned["content_height"], 320);
    let state = layout(&compositor);
    assert_eq!(state["pinned_windows"][0]["rect"]["x"], 1238.0);
    action(&compositor, format!("frame-reveal {id}"));
    assert_eq!(layout(&compositor)["pinned_windows"][0]["expanded"], true);
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    assert_eq!(client.focus(), Some(number));
    action(&compositor, "layout-tool");
    command(
        &compositor,
        "remove",
        json!({"kind":"window","id":id.to_string()}),
        Value::Null,
    );
    command(&compositor, "done", Value::Null, Value::Null);
    action(&compositor, format!("move-window {id} 320 250"));
    let moved = window(&compositor, "frame-layout-window");
    assert_eq!(moved["canvas_x"], 320);
    assert_eq!(moved["canvas_y"], 250);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_drag_cancel_restores_whole_group_and_keeps_clicks_out_of_clients() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut client = connect(&compositor);
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let wifi_surface = client.add(
        hearthspace_common::frame_layout::frame_widget_app_id(Some(&output), "wifi"),
        (44, 44),
        (0, 0),
        COLOR,
    );
    action(&compositor, "layout-tool");
    let before = layout(&compositor);
    let wifi = before["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == "wifi")
        .unwrap();
    let x = wifi["rect"]["x"].as_f64().unwrap() + 20.;
    let y = wifi["rect"]["y"].as_f64().unwrap() + 20.;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 400 350");
    keys(&compositor, &[1]);
    action(&compositor, "pointer-button-up 272");
    let after = layout(&compositor);
    assert_eq!(
        after["outputs"][0]["layout"],
        before["outputs"][0]["layout"]
    );
    assert_eq!(after["editing"], false);
    assert!(client.presses().is_empty());
    assert!(client.releases().is_empty());
    action(&compositor, "layout-tool");
    command(
        &compositor,
        "ungroup",
        json!({"kind":"widget","id":"wifi"}),
        Value::Null,
    );
    client.resize(wifi_surface, (54, 54));
    let separated = layout(&compositor);
    assert!(
        !separated["outputs"][0]["layout"]["groups"]
            .as_object()
            .unwrap()
            .values()
            .any(|g| g["members"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["kind"] == "widget" && m["id"] == "wifi")),
        "detached control merged back: {separated}"
    );
    command(&compositor, "done", Value::Null, Value::Null);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_restart_restores_app_slot_attachment_and_preferences() {
    let _guard = headless_test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    {
        let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
        drop(compositor.wait_for_socket());
        let mut client = connect(&compositor);
        client.add("frame-slot-test".into(), (500, 320), (0, 0), COLOR);
        pin(
            &compositor,
            window(&compositor, "frame-slot-test")["id"]
                .as_u64()
                .unwrap(),
        );
        action(&compositor, "frame-settings {\"allow_hide_tools\":true}");
        action(&compositor, "quit");
        wait_for_exit(&mut compositor.child);
    }
    let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(saved["frames"].as_object().unwrap().values().any(|l| {
        l["placements"]
            .as_object()
            .unwrap()
            .keys()
            .any(|key| key.starts_with("window:saved-"))
    }));
    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    assert_eq!(
        query(&compositor, "frame-settings-status")["allow_hide_tools"],
        true
    );
    let mut client = connect(&compositor);
    client.add("frame-slot-test".into(), (500, 320), (0, 0), COLOR);
    let status = layout(&compositor);
    assert_eq!(
        status["pinned_windows"].as_array().unwrap().len(),
        1,
        "{status}"
    );
    assert_eq!(status["pinned_windows"][0]["app_id"], "frame-slot-test");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_toolbar_turns_during_drag_and_returns_to_a_row_on_canvas_drop() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let tools = client.add(
        hearthspace_common::frame_layout::frame_widget_app_id(Some(&output), "tools"),
        (194, 54),
        (0, 0),
        COLOR,
    );
    action(&compositor, "layout-tool");
    let state = layout(&compositor);
    let tools_state = state["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == "tools")
        .unwrap();
    let x = tools_state["rect"]["x"].as_f64().unwrap() + 16.;
    let y = tools_state["rect"]["y"].as_f64().unwrap() + 20.;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 20 400");
    let vertical = layout(&compositor);
    let widget = vertical["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == "tools")
        .unwrap();
    assert_eq!(widget["orientation"], "vertical", "{vertical}");
    client.resize(tools, (54, 194));
    action(&compositor, "pointer-motion-abs 600 400");
    action(&compositor, "pointer-button-up 272");
    let dropped = layout(&compositor);
    let widget = dropped["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == "tools")
        .unwrap();
    assert_eq!(widget["orientation"], "horizontal", "{dropped}");
    assert_eq!(
        dropped["outputs"][0]["layout"]["placements"]["widget:tools"]["surface"],
        "canvas"
    );
    assert!(client.presses().is_empty());
    assert!(client.releases().is_empty());
    command(&compositor, "done", Value::Null, Value::Null);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
