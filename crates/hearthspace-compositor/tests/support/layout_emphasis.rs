//! Group feedback follows the real child surfaces without emitting events for every motion.
use super::navigation::{action, query};
use super::shell_panels::{click, connect};
use super::*;
use hearthspace_common::frame_layout::{frame_group_app_id, frame_widget_app_id};
use serde_json::Value;

fn group(compositor: &HeadlessCompositor, id: &str) -> Value {
    query(compositor, "layout-status")["outputs"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|group| group["id"] == id)
        .unwrap()
        .clone()
}

#[test]
fn layout_group_hover_and_focus_follow_members_and_stop_at_group_boundaries() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let initial = query(&compositor, "layout-status");
    let output = initial["outputs"][0]["output"].as_str().unwrap();
    let wifi = initial["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|widget| widget["id"] == "wifi")
        .unwrap();
    let group_id = wifi["group"].as_str().unwrap();
    let mut client = connect(&compositor);
    let bounds = group(&compositor, group_id)["rect"].clone();
    client.add(
        frame_group_app_id(Some(output), group_id),
        (
            bounds["width"].as_f64().unwrap().round() as i32,
            bounds["height"].as_f64().unwrap().round() as i32,
        ),
        (0, 0),
        [30, 40, 50, 255],
    );
    let wifi_surface = client.add(
        frame_widget_app_id(Some(output), "wifi"),
        (44, 44),
        (0, 0),
        [100, 130, 170, 255],
    );
    let x = wifi["rect"]["x"].as_f64().unwrap() + 20.;
    let y = wifi["rect"]["y"].as_f64().unwrap() + 20.;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    assert_eq!(group(&compositor, group_id)["hovered"], true);
    assert_eq!(group(&compositor, group_id)["focused"], false);
    let revision = query(&compositor, "layout-status")["revision"].clone();
    action(&compositor, format!("pointer-motion-abs {} {y}", x + 1.));
    assert_eq!(query(&compositor, "layout-status")["revision"], revision);

    click(&compositor, x, y);
    assert_eq!(client.focus(), Some(wifi_surface));
    assert_eq!(group(&compositor, group_id)["focused"], true);
    action(&compositor, "pointer-motion-abs 600 400");
    assert_eq!(group(&compositor, group_id)["hovered"], false);
    assert_eq!(group(&compositor, group_id)["focused"], true);
    client.add("layout-emphasis-normal".into(), (300, 200), (0, 0), [0; 4]);
    assert_eq!(group(&compositor, group_id)["focused"], false);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
