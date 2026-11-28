//! The real context/search boundary creates copies without moving existing widgets.
use super::navigation::{action, query};
use super::shell_panels::connect;
use super::*;
use hearthspace_common::frame_layout::frame_widget_app_id;
use serde_json::{Value, json};

fn layout(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "layout-status")
}
fn search(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "canvas-search-status")
}
fn copies(state: &Value, kind: &str) -> Vec<String> {
    state["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|widget| widget["kind"] == kind)
        .map(|widget| widget["id"].as_str().unwrap().to_owned())
        .collect()
}
fn right_click(compositor: &HeadlessCompositor, x: f64, y: f64) {
    action(compositor, format!("pointer-motion-abs {x} {y}"));
    action(compositor, "pointer-button-down 273");
    action(compositor, "pointer-button-up 273");
}
fn register(compositor: &HeadlessCompositor) {
    action(
        compositor,
        format!(
            "canvas-search-register {}",
            json!({"offset":0,"entries":[
                {"id":"widget:clock","title":"Add Clock","search_text":"clock widget time"},
                {"id":"widget:launcher","title":"Add App launcher","search_text":"launcher widget apps"},
                {"id":"control:clock","title":"Clock","search_text":"clock time"}
            ]})
        ),
    );
}
fn activate(compositor: &HeadlessCompositor, id: &str) {
    let session = search(compositor)["session"].as_u64().unwrap();
    action(
        compositor,
        format!(
            "canvas-search-activate {}",
            json!({"session":session,"id":id})
        ),
    );
}

#[test]
fn widget_picker_filters_without_a_query_token_and_adds_duplicates_at_the_original_canvas_point() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    register(&compositor);
    let before = layout(&compositor);
    let starter = copies(&before, "clock");
    assert_eq!(starter.len(), 1);
    action(&compositor, "canvas-tool pan");
    right_click(&compositor, 340.0, 280.0);
    let context = query(&compositor, "desktop-status")["context"].clone();
    let generation = context["generation"].as_u64().unwrap();
    action(&compositor, format!("desktop-context-widgets {generation}"));
    let picker = search(&compositor);
    assert_eq!(picker["filter"], "widgets");
    assert_eq!(picker["query"], "");
    assert_eq!(picker["results"].as_array().unwrap().len(), 2);
    assert!(
        picker["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|result| result["kind"] == "widget")
    );
    activate(&compositor, "widget:clock");
    assert_eq!(layout(&compositor)["canvas_tool"], "pan");
    let after = layout(&compositor);
    let added = copies(&after, "clock")
        .into_iter()
        .find(|id| !starter.contains(id))
        .unwrap();
    let placement = &after["outputs"][0]["layout"]["placements"][format!("widget:{added}")];
    assert_eq!(placement["surface"], "canvas");
    assert_eq!(placement["x"], 340.0);
    assert_eq!(placement["y"], 280.0);

    action(&compositor, "canvas-search-open");
    let session = search(&compositor)["session"].as_u64().unwrap();
    action(
        &compositor,
        format!(
            "canvas-search-update {}",
            json!({"session":session,"query":"clock"})
        ),
    );
    let regular = search(&compositor);
    assert!(
        regular["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|result| result["id"] == "control:clock")
    );
    assert!(
        regular["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|result| result["id"] == "widget:clock")
    );
    activate(&compositor, "widget:clock");
    let ids = copies(&layout(&compositor), "clock");
    assert_eq!(ids.len(), 3);
    assert_eq!(
        ids.iter().collect::<std::collections::BTreeSet<_>>().len(),
        3
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn a_canvas_widget_can_be_removed_and_a_stale_context_cannot_remove_its_replacement() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    register(&compositor);
    action(
        &compositor,
        "canvas-search-open {\"x\":340,\"y\":280,\"filter\":\"widgets\"}",
    );
    activate(&compositor, "widget:launcher");
    let initial = layout(&compositor);
    let id = copies(&initial, "launcher")[0].clone();
    let output = initial["outputs"][0]["output"].as_str().unwrap();
    let mut client = connect(&compositor);
    client.add(
        frame_widget_app_id(Some(output), &id),
        (54, 54),
        (0, 0),
        [0, 255, 0, 255],
    );
    right_click(&compositor, 360.0, 300.0);
    let context = query(&compositor, "desktop-status")["context"].clone();
    assert_eq!(context["ids"], json!([format!("widget:{id}")]));
    let generation = context["generation"].as_u64().unwrap();
    action(
        &compositor,
        format!("desktop-context-remove-widget {generation}"),
    );
    assert!(copies(&layout(&compositor), "launcher").is_empty());
    action(
        &compositor,
        "canvas-search-open {\"x\":340,\"y\":280,\"filter\":\"widgets\"}",
    );
    activate(&compositor, "widget:launcher");
    let replacement = copies(&layout(&compositor), "launcher")[0].clone();
    assert_ne!(replacement, id);
    assert!(
        compositor
            .send_text_command(&format!("desktop-context-remove-widget {generation}"))
            .starts_with("err ")
    );
    assert_eq!(copies(&layout(&compositor), "launcher"), [replacement]);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn removing_a_widget_during_a_drag_cannot_be_undone_by_the_later_release_or_editor_close() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    register(&compositor);
    action(
        &compositor,
        "canvas-search-open {\"x\":160,\"y\":220,\"filter\":\"widgets\"}",
    );
    activate(&compositor, "widget:launcher");
    let initial = layout(&compositor);
    let id = copies(&initial, "launcher")[0].clone();
    let output = initial["outputs"][0]["output"].as_str().unwrap().to_owned();
    let mut client = connect(&compositor);
    client.add(
        frame_widget_app_id(Some(&output), &id),
        (54, 54),
        (0, 0),
        [0, 255, 0, 255],
    );
    action(&compositor, "layout-tool");
    action(&compositor, "pointer-motion-abs 180 240");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 360 340");
    let session = layout(&compositor)["session"].as_u64().unwrap();
    action(
        &compositor,
        format!(
            "layout-command {}",
            json!({"session":session,"output":output,"action":"remove","target":{"kind":"widget","id":id}})
        ),
    );
    assert!(copies(&layout(&compositor), "launcher").is_empty());
    action(
        &compositor,
        format!(
            "layout-command {}",
            json!({"session":session,"output":output,"action":"done"})
        ),
    );
    action(&compositor, "pointer-button-up 272");
    assert!(copies(&layout(&compositor), "launcher").is_empty());
    action(
        &compositor,
        "canvas-search-open {\"x\":160,\"y\":220,\"filter\":\"widgets\"}",
    );
    activate(&compositor, "widget:launcher");
    let replacement = copies(&layout(&compositor), "launcher")[0].clone();
    assert_ne!(replacement, id);
    client.add(
        frame_widget_app_id(Some(&output), &replacement),
        (54, 54),
        (0, 0),
        [0, 0, 255, 255],
    );
    action(&compositor, "layout-tool");
    action(&compositor, "pointer-motion-abs 180 240");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 360 340");
    let before = layout(&compositor);
    let stale = format!(
        "layout-command {}",
        json!({"session":before["session"],"output":output,"action":"remove","target":{"kind":"widget","id":id}})
    );
    assert!(compositor.send_text_command(&stale).starts_with("err "));
    assert_eq!(
        layout(&compositor)["outputs"],
        before["outputs"],
        "a rejected stale removal must leave the replacement's drag intact"
    );
    action(&compositor, "pointer-motion-abs 400 360");
    assert_ne!(layout(&compositor)["outputs"], before["outputs"]);
    action(&compositor, "pointer-button-up 272");
    action(
        &compositor,
        format!(
            "layout-command {}",
            json!({"session":before["session"],"output":output,"action":"done"})
        ),
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
