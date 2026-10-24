//! Durable attachments outlive their client binding, except an explicit confirmed close.
use super::*;
use hearthspace_common::frame_layout::{
    FrameEdge, FrameLayout, LayoutTarget, ObjectPlacement, TargetKind,
};

#[path = "canonical_drop.rs"]
mod canonical_drop;

fn slot(app: &str, number: u8) -> LayoutTarget {
    let hex: String = app.bytes().map(|byte| format!("{byte:02x}")).collect();
    LayoutTarget::new(TargetKind::Window, format!("saved-{hex}-{number}"))
}

fn frame(state: &Value) -> FrameLayout {
    let layout: FrameLayout =
        serde_json::from_value(state["outputs"][0]["layout"].clone()).unwrap();
    layout.validate().unwrap();
    layout
}

fn saved_frame(path: &Path) -> FrameLayout {
    let saved: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let layout: FrameLayout =
        serde_json::from_value(saved["frames"]["hearthspace-0"].clone()).unwrap();
    layout.validate().unwrap();
    layout
}

fn pinned(state: &Value, id: u64) -> bool {
    state["pinned_windows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|pin| pin["id"] == id)
}

fn new_id(compositor: &HeadlessCompositor, app: &str) -> u64 {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|window| window["app_id"] == app)
        .map(|window| window["id"].as_u64().unwrap())
        .max()
        .unwrap()
}

fn drain_remember_passes(
    compositor: &HeadlessCompositor,
    client: &mut super::super::canvas_map_client::MapClient,
) {
    // Real protocol roundtrips cross event-loop boundaries. In the regression,
    // the first remember dropped the claim; the next forgot its attachment.
    for _ in 0..3 {
        client.sync();
        frame(&layout(compositor));
    }
}

#[test]
fn frame_layout_destroyed_client_keeps_slot_across_remember_passes_and_restart() {
    let _guard = headless_test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    let app = "dev.hearthspace.DurableFrame";
    {
        let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
        drop(compositor.wait_for_socket());
        let mut client = connect(&compositor);
        let number = client.add(app.into(), (500, 320), (0, 0), COLOR);
        let id = new_id(&compositor, app);
        pin(&compositor, id);
        assert!(pinned(&layout(&compositor), id));
        client.destroy(number);
        drain_remember_passes(&compositor, &mut client);
        assert!(
            layout(&compositor)["pinned_windows"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        action(&compositor, "quit");
        wait_for_exit(&mut compositor.child);
    }
    assert!(saved_frame(&path).placement(&slot(app, 0)).is_frame());
    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    client.add(
        "dev.hearthspace.Unrelated".into(),
        (300, 180),
        (0, 0),
        COLOR,
    );
    assert!(
        layout(&compositor)["pinned_windows"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let viewport = query(&compositor, "status")["viewport"].clone();
    client.add(app.into(), (600, 280), (0, 0), COLOR);
    let replacement = new_id(&compositor, app);
    let restored = layout(&compositor);
    assert!(pinned(&restored, replacement), "{restored}");
    assert!(
        frame(&restored)
            .placement(&LayoutTarget::new(
                TargetKind::Window,
                replacement.to_string()
            ))
            .is_frame()
    );
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_grouped_replacement_reuses_vacant_app_slot_and_confirmed_close_forgets_it() {
    let _guard = headless_test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    let app = "dev.hearthspace.GroupedFrame";
    let apps = LayoutTarget::widget("apps");
    let durable = slot(app, 0);
    let mut original = FrameLayout::default();
    // Detaching the tall window uses the upper right edge. Keep the unrelated
    // system bar clear of that strip so its valid auto-merge cannot mask detach.
    original.set_placement(
        &LayoutTarget::new(TargetKind::Group, "system"),
        ObjectPlacement::frame(FrameEdge::Top, 0.5),
    );
    original.set_placement(&apps, ObjectPlacement::frame(FrameEdge::Right, 0.5));
    let group = original.merge(&durable, &apps, false).unwrap();
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "version":1,"x":2500,"y":-1700,"scale":1.0,"placements":[],
            "frames":{"hearthspace-0":original}
        }))
        .unwrap(),
    )
    .unwrap();
    {
        let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
        drop(compositor.wait_for_socket());
        let mut client = connect(&compositor);
        let first = client.add(app.into(), (500, 320), (0, 0), COLOR);
        let first_id = new_id(&compositor, app);
        client.add(app.into(), (300, 220), (0, 0), COLOR);
        let second_id = new_id(&compositor, app);
        assert!(pinned(&layout(&compositor), first_id));
        assert!(!pinned(&layout(&compositor), second_id));
        client.destroy(first);
        drain_remember_passes(&compositor, &mut client);
        let absent = layout(&compositor);
        assert!(!pinned(&absent, first_id));
        assert!(!pinned(&absent, second_id));
        assert!(frame(&absent).groups.contains_key(&group.id));
        let viewport = query(&compositor, "status")["viewport"].clone();
        let replacement = client.add(app.into(), (460, 260), (0, 0), COLOR);
        let replacement_id = new_id(&compositor, app);
        let state = layout(&compositor);
        assert!(pinned(&state, replacement_id), "{state}");
        assert!(!pinned(&state, second_id));
        let target = LayoutTarget::new(TargetKind::Window, replacement_id.to_string());
        assert_eq!(frame(&state).group_for(&target).unwrap().id, group.id);
        assert_eq!(query(&compositor, "status")["viewport"], viewport);

        // Runtime commands resolve to the durable group member, including edits
        // that detach it. A stale destroyed ID must not target its replacement.
        action(&compositor, "layout-tool");
        let session = layout(&compositor);
        assert!(
            compositor
                .send_text_command(&format!(
                    "layout-command {}",
                    json!({
                        "session":session["session"],"output":session["output"],"action":"remove",
                        "target":{"kind":"window","id":first_id.to_string()}
                    })
                ))
                .starts_with("err ")
        );
        command(
            &compositor,
            "ungroup",
            serde_json::to_value(&target).unwrap(),
            Value::Null,
        );
        assert!(frame(&layout(&compositor)).group_for(&target).is_none());
        command(
            &compositor,
            "place",
            serde_json::to_value(&target).unwrap(),
            json!({"surface":"frame","edge":"right","position":0.5}),
        );
        assert!(frame(&layout(&compositor)).group_for(&target).is_some());
        command(&compositor, "done", Value::Null, Value::Null);
        action(&compositor, format!("close-window {replacement_id}"));
        assert_eq!(layout(&compositor)["confirmation"]["kind"], "close-window");
        command(&compositor, "confirm", Value::Null, Value::Null);
        assert!(client.closed().contains(&replacement));
        client.destroy(replacement);
        drain_remember_passes(&compositor, &mut client);
        let removed = frame(&layout(&compositor));
        assert!(removed.placement(&apps).is_frame());
        assert!(!removed.placement(&durable).is_frame());
        action(&compositor, "quit");
        wait_for_exit(&mut compositor.child);
    }
    let saved = saved_frame(&path);
    assert!(!saved.placement(&durable).is_frame());
    assert!(saved.placement(&apps).is_frame());
    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    client.add(app.into(), (500, 320), (0, 0), COLOR);
    assert!(!pinned(&layout(&compositor), new_id(&compositor, app)));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_wayland_unmap_is_invisible_and_remap_keeps_the_same_slot() {
    let _guard = headless_test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut client = super::super::interactive::client::InteractiveClient::connect(&socket);
    let windows = query(&compositor, "windows");
    let id = windows["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["title"] == "Interactive fixture 0")
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    pin(&compositor, id);
    client.unmap(0);
    for _ in 0..3 {
        client.sync();
        frame(&layout(&compositor));
    }
    assert!(!pinned(&layout(&compositor), id));
    let viewport = query(&compositor, "status")["viewport"].clone();
    client.remap(0);
    assert!(pinned(&layout(&compositor), id));
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    client.unmap(0);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
    assert!(
        saved_frame(&path)
            .placement(&slot("dev.hearthspace.InteractiveTest", 0))
            .is_frame()
    );
}
