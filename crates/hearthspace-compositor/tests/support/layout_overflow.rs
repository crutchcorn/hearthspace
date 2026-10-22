//! Compact defaults remain temporary, and clipped group members stay reachable.
use super::navigation::{action, query};
use super::shell_panels::{click, connect};
use super::*;
use hearthspace_common::frame_layout::{
    FrameEdge, FrameGroup, FrameLayout, LayoutTarget, ObjectPlacement, WIDGETS, frame_group_app_id,
    frame_widget_app_id,
};
use serde_json::{Value, json};

fn layout(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "layout-status")
}
fn widget(compositor: &HeadlessCompositor, id: &str) -> Value {
    layout(compositor)["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == id)
        .unwrap()
        .clone()
}

#[test]
fn layout_compact_default_bars_do_not_rewrite_groups_or_custom_placements() {
    let _guard = headless_test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    let mut compositor = HeadlessCompositor::spawn_with_layout("700x600", Some(&path));
    drop(compositor.wait_for_socket());
    let before = layout(&compositor);
    let output = &before["outputs"][0];
    assert_eq!(
        output["rect"],
        json!({"x":0.0,"y":0.0,"width":700.0,"height":600.0}),
        "standalone native viewports need the full output bounds, not their previous size"
    );
    assert_eq!(output["compact"], true);
    assert_eq!(
        output["layout"],
        serde_json::to_value(FrameLayout::default()).unwrap()
    );
    let bars: Vec<_> = output["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["backing_only"] == true)
        .collect();
    assert_eq!(bars.len(), 2);
    for bar in bars {
        assert_eq!(bar["rect"]["x"], 12.);
        assert_eq!(bar["rect"]["width"], 676.);
        assert_eq!(bar["rect"]["height"], 54.);
    }
    assert_eq!(widget(&compositor, "notifications")["merged"], true);
    action(&compositor, "layout-tool");
    let editing = layout(&compositor);
    action(
        &compositor,
        format!(
            "layout-command {}",
            json!({"session":editing["session"],"output":editing["output"],"action":"place","target":{"kind":"widget","id":"notifications"},"placement":{"surface":"canvas","x":240.,"y":200.}})
        ),
    );
    let changed = layout(&compositor);
    assert_eq!(widget(&compositor, "notifications")["merged"], false);
    assert!(
        !changed["outputs"][0]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["id"] == "@compact-top")
    );
    assert!(
        changed["outputs"][0]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["id"] == "@compact-bottom")
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
    let persisted = std::fs::read_to_string(path).unwrap();
    assert!(
        !persisted.contains("@compact"),
        "responsive backing leaked into saved layout"
    );
}

#[test]
fn layout_overflow_wheel_and_native_focus_reveal_members_without_canvas_navigation() {
    let _guard = headless_test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    let mut frame = FrameLayout {
        allow_hide_tools: true,
        ..FrameLayout::default()
    };
    frame.groups.clear();
    frame.placements.clear();
    let members = ["clock", "wifi", "volume", "power"];
    for id in WIDGETS.iter().filter(|id| !members.contains(id)) {
        frame
            .placements
            .insert(LayoutTarget::widget(*id).key(), ObjectPlacement::Hidden);
    }
    frame.groups.insert(
        "long".into(),
        FrameGroup {
            id: "long".into(),
            members: members.iter().map(|id| LayoutTarget::widget(*id)).collect(),
        },
    );
    frame.placements.insert(
        "group:long".into(),
        ObjectPlacement::frame(FrameEdge::Top, 0.5),
    );
    std::fs::write(&path, serde_json::to_vec(&json!({"version":1,"x":0,"y":0,"scale":1.,"placements":[],"frames":{"hearthspace-0":frame},"allow_hide_tools":true})).unwrap()).unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    client.add(
        frame_group_app_id(Some(&output), "long"),
        (1232, 54),
        (0, 0),
        [20, 30, 40, 255],
    );
    let mut power = 0;
    for id in members {
        let number = client.add(
            frame_widget_app_id(Some(&output), id),
            (if id == "power" { 1500 } else { 600 }, 44),
            (0, 0),
            [50, 90, 150, 255],
        );
        if id == "power" {
            power = number;
        }
    }
    let viewport = query(&compositor, "status")["viewport"].clone();
    let before = widget(&compositor, "clock")["rect"]["x"].as_f64().unwrap();
    action(&compositor, "pointer-motion-abs 100 45");
    action(&compositor, "axis 500 0");
    assert!(widget(&compositor, "clock")["rect"]["x"].as_f64().unwrap() < before);
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    click(&compositor, 26., 45.);
    assert!(
        client.presses().iter().all(|(number, _)| *number == 0),
        "clipped member accepted a click outside its viewport"
    );
    action(
        &compositor,
        format!(
            "frame-focus-next {}",
            json!({"output":output,"target":{"kind":"widget","id":"volume"}})
        ),
    );
    assert_eq!(client.focus(), Some(power));
    // The compositor may constrain the requested surface to the output width.
    // Locate the last native control inside the mapped client, not its original
    // requested size, and prove that native focus reveals this clipped region.
    let power_rect = widget(&compositor, "power")["rect"].clone();
    let control_x = power_rect["width"].as_f64().unwrap() - 50.;
    let group = layout(&compositor)["outputs"][0]["groups"][0]["rect"].clone();
    let visible_left = group["x"].as_f64().unwrap() + 5.;
    let visible_right = group["x"].as_f64().unwrap() + group["width"].as_f64().unwrap() - 5.;
    assert!(power_rect["x"].as_f64().unwrap() + control_x + 44. > visible_right);
    action(
        &compositor,
        format!(
            "frame-focus-region {}",
            json!({"output":output,"target":{"kind":"widget","id":"power"},"rect":{"x":control_x,"y":0.,"width":44.,"height":44.}})
        ),
    );
    let power_rect = widget(&compositor, "power")["rect"].clone();
    let left = power_rect["x"].as_f64().unwrap() + control_x;
    assert!(
        left >= visible_left && left + 44. <= visible_right,
        "focused final control was not revealed: {power_rect}"
    );
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
