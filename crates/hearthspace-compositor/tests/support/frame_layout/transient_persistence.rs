//! Registered under interactive_client; reuses its SHM dispatch for a real dialog.
use super::*;
use crate::{
    HeadlessCompositor, headless_test_guard,
    navigation::{action, query},
    wait_for_exit,
};
use hearthspace_common::frame_layout::{FrameLayout, LayoutTarget, TargetKind};
use serde_json::{Value, json};

fn add_dialog(client: &mut InteractiveClient) -> usize {
    let handle = client.queue.handle();
    let number = client.state.windows.len();
    let surface = client
        .state
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&handle, ());
    let xdg_surface = client
        .state
        .wm_base
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &handle, number);
    let toplevel = xdg_surface.get_toplevel(&handle, number);
    toplevel.set_app_id("dev.hearthspace.InteractiveTest".into());
    toplevel.set_title("Transient frame fixture".into());
    toplevel.set_parent(Some(&client.state.windows[0].toplevel));
    client.state.windows.push(Window {
        surface: surface.clone(),
        xdg_surface,
        toplevel,
        size: (240, 160),
        inset: (0, 0),
        configure_count: 0,
        resizing: false,
        mapped: true,
    });
    surface.commit();
    client.sync();
    number
}

#[test]
fn frame_layout_transient_pin_is_live_only_and_cannot_occupy_its_parent_app_slot() {
    let _guard = headless_test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    {
        let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
        drop(compositor.wait_for_socket());
        let socket = compositor
            .runtime_dir
            .path()
            .join(format!("hearthspace-test-{}", std::process::id()));
        let mut client = InteractiveClient::connect(&socket);
        add_dialog(&mut client);
        let windows = query(&compositor, "windows");
        let dialog = windows["windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|window| window["title"] == "Transient frame fixture")
            .unwrap()["id"]
            .as_u64()
            .unwrap();
        action(&compositor, "layout-tool");
        let status = query(&compositor, "layout-status");
        action(
            &compositor,
            format!(
                "layout-command {}",
                json!({
                    "session":status["session"],"output":status["output"],"action":"place",
                    "target":{"kind":"window","id":dialog.to_string()},
                    "placement":{"surface":"frame","edge":"right","position":0.5}
                })
            ),
        );
        action(
            &compositor,
            format!(
                "layout-command {}",
                json!({
                    "session":status["session"],"output":status["output"],"action":"done"
                })
            ),
        );
        assert!(
            query(&compositor, "layout-status")["pinned_windows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|pin| pin["id"] == dialog)
        );
        // Save while the transient is live: teardown must not be what prevents
        // its runtime ID from leaking into the next compositor session.
        action(&compositor, "quit");
        wait_for_exit(&mut compositor.child);
    }
    let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let layout: FrameLayout =
        serde_json::from_value(saved["frames"]["hearthspace-0"].clone()).unwrap();
    layout.validate().unwrap();
    assert!(
        layout
            .placements
            .keys()
            .filter_map(|key| LayoutTarget::from_key(key))
            .chain(
                layout
                    .groups
                    .values()
                    .flat_map(|group| group.members.clone())
            )
            .all(|target| target.kind != TargetKind::Window)
    );
    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut client = InteractiveClient::connect(&socket);
    add_dialog(&mut client);
    assert!(
        query(&compositor, "layout-status")["pinned_windows"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_transient_runtime_background_drop_stays_fixed_under_foreground() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut client = InteractiveClient::connect(&socket);
    let windows = query(&compositor, "windows");
    for (number, x, y) in [(0, -1500, -1000), (1, 400, 300)] {
        let id = windows["windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|window| window["title"] == format!("Interactive fixture {number}"))
            .unwrap()["id"]
            .as_u64()
            .unwrap();
        action(&compositor, format!("move-window {id} {x} {y}"));
    }
    add_dialog(&mut client);
    let windows = query(&compositor, "windows");
    let dialog = windows["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["title"] == "Transient frame fixture")
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let viewport = query(&compositor, "status")["viewport"].clone();
    action(&compositor, "layout-tool");
    for placement in [
        json!({"surface":"frame","edge":"right","position":0.5}),
        json!({"surface":"background","x":400.0,"y":300.0}),
    ] {
        let state = query(&compositor, "layout-status");
        action(
            &compositor,
            format!(
                "layout-command {}",
                json!({
                    "session":state["session"],"output":state["output"],"action":"place",
                    "target":{"kind":"window","id":dialog.to_string()},"placement":placement
                })
            ),
        );
    }
    for _ in 0..3 {
        client.sync();
        let windows = query(&compositor, "windows");
        for title in ["Transient frame fixture", "Interactive fixture 1"] {
            let window = windows["windows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|window| window["title"] == title)
                .unwrap();
            assert_eq!(window["canvas_x"], 400);
            assert_eq!(window["canvas_y"], 300);
            assert_eq!(window["content_width"], 240);
            assert_eq!(window["content_height"], 160);
        }
        assert_eq!(query(&compositor, "status")["viewport"], viewport);
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
