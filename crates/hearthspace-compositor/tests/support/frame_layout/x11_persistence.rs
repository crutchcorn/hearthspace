//! Registered under x11_interactive to reuse its real X11 client and barriers.
use super::*;
use hearthspace_common::frame_layout::{FrameLayout, LayoutTarget, TargetKind};
use serde_json::{Value, json};
use smithay::reexports::x11rb::{connection::Connection, protocol::xproto::ConnectionExt};

fn layout(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "layout-status")
}

fn pinned(compositor: &HeadlessCompositor, id: u64) -> bool {
    layout(compositor)["pinned_windows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|pin| pin["id"] == id)
}

fn pin(compositor: &HeadlessCompositor, id: u64) {
    action(compositor, "layout-tool");
    let state = layout(compositor);
    action(
        compositor,
        format!(
            "layout-command {}",
            json!({
                "session":state["session"],"output":state["output"],"action":"place",
                "target":{"kind":"window","id":id.to_string()},
                "placement":{"surface":"frame","edge":"right","position":0.5}
            })
        ),
    );
    action(
        compositor,
        format!(
            "layout-command {}",
            json!({
                "session":state["session"],"output":state["output"],"action":"done"
            })
        ),
    );
}

fn remember_passes(
    compositor: &HeadlessCompositor,
    client: &mut super::super::canvas_map_client::MapClient,
) {
    for _ in 0..3 {
        client.sync();
        let _ = layout(compositor);
    }
}

#[test]
fn frame_layout_x11_unmap_rebinds_new_runtime_id_and_destruction_preserves_saved_slot() {
    let _guard = headless_test_guard();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    let app;
    {
        let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
        drop(compositor.wait_for_socket());
        let mut client = Client::connect(&compositor);
        let mut barrier_client = super::super::shell_panels::connect(&compositor);
        app = query(&compositor, "windows")["windows"][0]["app_id"]
            .as_str()
            .unwrap()
            .to_owned();
        pin(&compositor, client.id);
        assert!(pinned(&compositor, client.id));
        let old = client.id;
        client
            .connection
            .unmap_window(client.window)
            .unwrap()
            .check()
            .unwrap();
        wait_windows(&compositor, |windows| windows.is_empty());
        remember_passes(&compositor, &mut barrier_client);
        assert!(
            layout(&compositor)["pinned_windows"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let viewport = query(&compositor, "status")["viewport"].clone();
        client.connection.map_window(client.window).unwrap();
        client.connection.flush().unwrap();
        let windows = wait_windows(&compositor, |windows| {
            windows.len() == 1 && windows[0]["mapped"] == true
        });
        client.id = windows[0]["id"].as_u64().unwrap();
        assert_ne!(old, client.id);
        assert!(pinned(&compositor, client.id));
        assert_eq!(query(&compositor, "status")["viewport"], viewport);
        client
            .connection
            .destroy_window(client.window)
            .unwrap()
            .check()
            .unwrap();
        wait_windows(&compositor, |windows| windows.is_empty());
        remember_passes(&compositor, &mut barrier_client);
        action(&compositor, "quit");
        wait_for_exit(&mut compositor.child);
    }
    let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let frames: FrameLayout =
        serde_json::from_value(saved["frames"]["hearthspace-0"].clone()).unwrap();
    let hex: String = app.bytes().map(|byte| format!("{byte:02x}")).collect();
    assert!(
        frames
            .placement(&LayoutTarget::new(
                TargetKind::Window,
                format!("saved-{hex}-0")
            ))
            .is_frame()
    );
    frames.validate().unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    // Slots identify the app, not the document or display protocol. The X11
    // fixture's constructor explicitly moves its window, which attached clients
    // reject; use the existing Wayland fixture for this replacement instead.
    let mut client = super::super::shell_panels::connect(&compositor);
    client.add(app.clone(), (320, 240), (0, 0), [220, 20, 60, 255]);
    let windows = wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["app_id"] == app
    });
    assert!(pinned(&compositor, windows[0]["id"].as_u64().unwrap()));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
