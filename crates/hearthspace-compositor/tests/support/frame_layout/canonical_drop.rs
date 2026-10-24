//! A saved group release is a deliberate live drop, including after member retirement.
use super::*;

#[test]
fn frame_layout_saved_group_background_drop_keeps_survivor_fixed_under_foreground() {
    let _guard = headless_test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    let absent_app = "dev.hearthspace.AbsentGroupDrop";
    let live_app = "dev.hearthspace.LiveGroupDrop";
    let cover_app = "dev.hearthspace.GroupDropCover";
    let absent = slot(absent_app, 0);
    let live = slot(live_app, 0);
    let mut original = FrameLayout::default();
    original.set_placement(&live, ObjectPlacement::frame(FrameEdge::Right, 0.5));
    // Keep the unavailable member last: attempting to settle its retired ID
    // after the live drop would incorrectly fall back to ordinary settlement.
    let group = original.merge(&absent, &live, false).unwrap();
    original.validate().unwrap();
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "version":1,"x":0,"y":0,"scale":1.0,"placements":[],
            "frames":{"hearthspace-0":original}
        }))
        .unwrap(),
    )
    .unwrap();

    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut client = connect(&compositor);
    let retired = client.add(absent_app.into(), (240, 160), (0, 0), COLOR);
    let retired_id = new_id(&compositor, absent_app);
    client.add(live_app.into(), (240, 160), (0, 0), COLOR);
    let id = new_id(&compositor, live_app);
    assert!(pinned(&layout(&compositor), retired_id));
    assert!(pinned(&layout(&compositor), id));
    client.destroy(retired);
    drain_remember_passes(&compositor, &mut client);
    let state = layout(&compositor);
    assert!(!pinned(&state, retired_id));
    let projected = frame(&state);
    let members = &projected.groups[&group.id].members;
    assert_eq!(
        members,
        &[
            LayoutTarget::new(TargetKind::Window, id.to_string()),
            absent.clone()
        ]
    );
    let rect = state["pinned_windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pin| pin["id"] == id)
        .unwrap()["rect"]
        .clone();
    let x = rect["x"].as_f64().unwrap().round() as i64;
    let y = rect["y"].as_f64().unwrap().round() as i64;

    // Cover the complete native window at the released slot, not just its
    // narrow pinned strip. Ordinary settlement would move it to expose an edge.
    client.add(cover_app.into(), (260, 180), (0, 0), COLOR);
    let cover = new_id(&compositor, cover_app);
    action(
        &compositor,
        format!("move-window {cover} {} {}", x - 10, y - 10),
    );
    let foreground = window(&compositor, cover_app);
    let viewport = query(&compositor, "status")["viewport"].clone();
    action(&compositor, "layout-tool");
    command(
        &compositor,
        "place",
        serde_json::to_value(&group).unwrap(),
        json!({"surface":"background"}),
    );
    command(&compositor, "done", Value::Null, Value::Null);
    for _ in 0..3 {
        client.sync();
        let released = window(&compositor, live_app);
        assert_eq!(released["id"], id);
        assert_eq!(
            released["canvas_x"], x,
            "a deliberate drop must remain fixed"
        );
        assert_eq!(
            released["canvas_y"], y,
            "a deliberate drop must remain fixed"
        );
        assert_eq!(released["content_width"], 240);
        assert_eq!(released["content_height"], 160);
        let cover = window(&compositor, cover_app);
        assert_eq!(cover["canvas_x"], foreground["canvas_x"]);
        assert_eq!(cover["canvas_y"], foreground["canvas_y"]);
        assert_eq!(query(&compositor, "status")["viewport"], viewport);
    }
    let released = frame(&layout(&compositor));
    assert!(!released.groups.contains_key(&group.id));
    assert!(matches!(
        released.placement(&LayoutTarget::new(TargetKind::Window, id.to_string())),
        ObjectPlacement::Background { .. }
    ));
    assert!(matches!(
        released.placement(&absent),
        ObjectPlacement::Background { .. }
    ));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
    let saved = saved_frame(&path);
    assert!(matches!(
        saved.placement(&live),
        ObjectPlacement::Background { .. }
    ));
    assert!(matches!(
        saved.placement(&absent),
        ObjectPlacement::Background { .. }
    ));
}
