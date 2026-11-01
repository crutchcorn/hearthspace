//! Metadata changes and null-buffer role reset must reach placement and hit testing.
use super::super::{canvas_map_client::MapClient, shell_panels::click};
use super::*;
use hearthspace_common::{desktop_item_app_id, frame_layout::*};

fn fixture() -> (HeadlessCompositor, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    let mut frame = FrameLayout::default();
    frame.groups.clear();
    frame.placements.clear();
    for (id, members, position) in [
        ("alpha", ["wifi", "volume"], 0.2),
        ("beta", ["clock", "battery"], 0.8),
    ] {
        frame.groups.insert(
            id.into(),
            FrameGroup {
                id: id.into(),
                members: members.into_iter().map(LayoutTarget::widget).collect(),
            },
        );
        frame.placements.insert(
            format!("group:{id}"),
            ObjectPlacement::frame(FrameEdge::Top, position),
        );
    }
    frame.validate().unwrap();
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "version":1,"x":0,"y":0,"scale":1.,"placements":[],
            "frames":{"hearthspace-0":frame}
        }))
        .unwrap(),
    )
    .unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    (compositor, directory)
}

fn frame_id(family: &str, target: &str) -> String {
    match family {
        "widget" => frame_widget_app_id(Some("hearthspace-0"), target),
        "group" => frame_group_app_id(Some("hearthspace-0"), target),
        "grip" => frame_group_grip_app_id(Some("hearthspace-0"), target),
        _ => unreachable!(),
    }
}

fn target_point(compositor: &HeadlessCompositor, family: &str, target: &str) -> (f64, f64) {
    let state = layout(compositor);
    let list = if family == "widget" {
        "widgets"
    } else {
        "groups"
    };
    let item = state["outputs"][0][list]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == target)
        .unwrap();
    let rect = &item[if family == "grip" { "grip" } else { "rect" }];
    (
        rect["x"].as_f64().unwrap() + 10.,
        rect["y"].as_f64().unwrap() + 10.,
    )
}

fn point_at(compositor: &HeadlessCompositor, point: (f64, f64)) {
    action(compositor, "pointer-motion-abs 600 400");
    action(
        compositor,
        format!("pointer-motion-abs {} {}", point.0, point.1),
    );
}

fn assert_target(
    compositor: &HeadlessCompositor,
    client: &mut MapClient,
    surface: usize,
    family: &str,
    target: &str,
    group: &str,
) {
    let point = target_point(compositor, family, target);
    point_at(compositor, point);
    assert_eq!(
        client.pointer_target(),
        Some(surface),
        "{family}:{target} hit testing"
    );
    let state = layout(compositor);
    for item in state["outputs"][0]["groups"].as_array().unwrap() {
        assert_eq!(item["hovered"], item["id"] == group, "{state}");
    }
    click(compositor, point.0, point.1);
    let kind = if family == "widget" {
        "widget"
    } else {
        "group"
    };
    assert_eq!(
        layout(compositor)["selected"]["target"],
        json!({"kind":kind,"id":target})
    );
}

#[rstest::rstest]
#[case::widget("widget", "wifi", "clock")]
#[case::group("group", "alpha", "beta")]
#[case::grip("grip", "alpha", "beta")]
fn cached_frame_identity_tracks_same_kind_changes_and_unmap_role_reset(
    #[case] family: &str,
    #[case] first: &str,
    #[case] second: &str,
) {
    let _guard = headless_test_guard();
    let (mut compositor, _directory) = fixture();
    let mut client = connect(&compositor);
    let invalid = "dev.hearthspace.frame-widget.target-zz";
    let surface = client.add(invalid.into(), (44, 44), (0, 0), COLOR);
    let managed = window(&compositor, invalid)["id"].clone();
    action(&compositor, "layout-tool");
    client.capture_app_id(surface, &frame_id(family, first));
    assert_target(&compositor, &mut client, surface, family, first, "alpha");
    client.capture_app_id(surface, &frame_id(family, second));
    assert_target(&compositor, &mut client, surface, family, second, "beta");

    let old_point = target_point(&compositor, family, second);
    client.unmap(surface);
    point_at(&compositor, old_point);
    assert_ne!(client.pointer_target(), Some(surface));
    // Smithay resets app_id without calling app_id_changed. The retained
    // generic ShellBar can cover the old point, but has no frame group target.
    // Renderer-free protocol tests check syntax absence directly, independent
    // of this hit-test path's separate raw-ID visibility guard.
    client.remap_without_app_id(surface);
    point_at(&compositor, (20., 20.));
    assert_eq!(
        client.pointer_target(),
        Some(surface),
        "no-ID remap must deliver pointer events inside the generic bar"
    );
    point_at(&compositor, old_point);
    assert!(
        layout(&compositor)["outputs"][0]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .all(|group| group["hovered"] == false)
    );
    // Size hints alone leave the generic bar's full-width buffer/geometry live.
    // Commit control geometry before restoring identity, or frame collision
    // settlement can merge its group before the new configure is acknowledged.
    client.repaint_with_size(surface, (44, 44));
    client.capture_app_id(surface, &frame_id(family, second));
    assert_target(&compositor, &mut client, surface, family, second, "beta");

    client.capture_app_id(surface, invalid);
    assert_eq!(window(&compositor, invalid)["id"], managed);
    point_at(&compositor, old_point);
    assert_ne!(client.pointer_target(), Some(surface));
    client.destroy(surface);
    assert!(
        query(&compositor, "windows")["windows"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

fn assert_desktop_target(compositor: &HeadlessCompositor, id: &str) {
    let items = query(compositor, "desktop-status")["items"].clone();
    assert_eq!(items.as_array().unwrap().len(), 1);
    assert_eq!(items[0]["id"], id);
    command(
        compositor,
        "select",
        json!({"kind":"item","id":id}),
        Value::Null,
    );
    assert_eq!(
        layout(compositor)["selected"]["target"],
        json!({"kind":"item","id":id})
    );
}

#[test]
fn cached_desktop_identity_tracks_current_target_and_remap_without_app_id() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let first = "file:61";
    let second = "file:62";
    let surface = client.add(desktop_item_app_id(first), (100, 108), (0, 0), COLOR);
    action(&compositor, "layout-tool");
    assert_desktop_target(&compositor, first);
    client.capture_app_id(surface, &desktop_item_app_id(second));
    assert_desktop_target(&compositor, second);
    let state = layout(&compositor);
    let stale = format!(
        "layout-command {}",
        json!({
            "session":state["session"],"output":state["output"],
            "action":"select","target":{"kind":"item","id":first}
        })
    );
    assert!(compositor.send_text_command(&stale).starts_with("err "));

    client.unmap(surface);
    assert!(
        query(&compositor, "desktop-status")["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    client.remap_without_app_id(surface);
    assert!(
        query(&compositor, "desktop-status")["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    client.capture_app_id(surface, &desktop_item_app_id(first));
    assert_desktop_target(&compositor, first);
    client.capture_app_id(surface, "dev.hearthspace.desktop-item.item-zz");
    assert!(
        query(&compositor, "desktop-status")["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn cached_other_syntax_does_not_retain_layout_session_authorization() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    action(&compositor, "layout-tool");
    let opened = layout(&compositor);
    let session = opened["session"].as_u64().unwrap();
    let output = opened["output"].as_str().unwrap();
    let mut client = connect(&compositor);
    let editor = client.add(
        layout_editor_app_id(session, Some(output)),
        (260, 156),
        (0, 0),
        COLOR,
    );
    let point = (530., 110.);
    point_at(&compositor, point);
    assert_eq!(client.pointer_target(), Some(editor));
    command(&compositor, "done", Value::Null, Value::Null);
    point_at(&compositor, point);
    assert_ne!(client.pointer_target(), Some(editor));

    action(&compositor, "layout-tool");
    let next = layout(&compositor)["session"].as_u64().unwrap();
    assert_ne!(next, session);
    point_at(&compositor, point);
    assert_ne!(client.pointer_target(), Some(editor));
    client.capture_app_id(editor, &layout_editor_app_id(next, Some(output)));
    point_at(&compositor, point);
    assert_eq!(client.pointer_target(), Some(editor));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
