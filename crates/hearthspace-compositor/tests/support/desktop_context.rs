//! Real desktop surfaces exercise menu ownership, generations, and output anchoring.
use super::*;
use super::{
    canvas_map_client::MapClient,
    navigation::{action, keys, query},
    shell_panels::{click, connect},
};
use hearthspace_common::desktop_context;
use serde_json::{Value, json};

#[path = "desktop_context/apps.rs"]
mod apps;
#[path = "desktop_context/submenus.rs"]
mod submenus;

fn fixture() -> (HeadlessCompositor, MapClient, usize, [usize; 2]) {
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let normal = client.add("context-app".into(), (300, 240), (0, 0), [0, 0, 255, 255]);
    let id = query(&compositor, "windows")["windows"][0]["id"]
        .as_u64()
        .unwrap();
    action(&compositor, format!("move-window {id} 600 240"));
    let items = ["file:61", "file:62"].map(|id| {
        client.add(
            hearthspace_common::desktop_item_app_id(id),
            (100, 108),
            (0, 0),
            [0, 255, 0, 255],
        )
    });
    action(
        &compositor,
        "desktop-register {\"offset\":0,\"ids\":[\"file:61\",\"file:62\"]}",
    );
    action(
        &compositor,
        "desktop-file-register {\"offset\":0,\"entries\":[{\"id\":\"file:61\",\"uri\":\"file:///tmp/context-a\"},{\"id\":\"file:62\",\"uri\":\"file:///tmp/context-b\"}]}",
    );
    (compositor, client, normal, items)
}

fn context(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "desktop-status")["context"].clone()
}
fn point(compositor: &HeadlessCompositor, id: &str) -> (f64, f64) {
    let status = query(compositor, "desktop-status");
    let item = status["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == id)
        .unwrap();
    (
        item["x"].as_f64().unwrap() + 44.0,
        item["y"].as_f64().unwrap() + 40.0,
    )
}
fn right_click(compositor: &HeadlessCompositor, point: (f64, f64)) {
    action(
        compositor,
        format!("pointer-motion-abs {} {}", point.0, point.1),
    );
    action(compositor, "pointer-button-down 273");
    action(compositor, "pointer-button-up 273");
}
fn map_menu(client: &mut MapClient, context: &Value) -> usize {
    client.add(
        desktop_context::app_id(
            context["generation"].as_u64().unwrap(),
            context["output"].as_str(),
        ),
        (210, 170),
        (8, 6),
        [255, 0, 0, 255],
    )
}
fn close(compositor: &HeadlessCompositor, context: &Value) {
    action(
        compositor,
        format!("desktop-context-close {}", context["generation"]),
    );
}
fn panel(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "status")["shell_panels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["kind"] == "desktop-context")
        .expect("visible context menu")
        .clone()
}
fn finish(compositor: &mut HeadlessCompositor) {
    action(compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn desktop_context_preserves_multiselection_and_restores_the_clicked_item() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, normal, items) = fixture();
    right_click(&compositor, (650.0, 330.0));
    assert!(
        context(&compositor).is_null(),
        "ordinary applications own their context menus"
    );
    assert_eq!(client.presses().last().unwrap().0, normal);
    assert_eq!(client.releases().last().unwrap().0, normal);

    action(&compositor, "desktop-select \"file:61\"");
    action(&compositor, "key-down 29");
    let p = point(&compositor, "file:62");
    click(&compositor, p.0, p.1);
    action(&compositor, "key-up 29");
    let presses = client.presses().len();
    right_click(&compositor, p);
    let menu = context(&compositor);
    assert_eq!(menu["ids"], json!(["file:61", "file:62"]));
    assert_eq!(
        client.presses().len(),
        presses,
        "opening the menu must not click a tile"
    );
    for (command, cut) in [
        ("desktop-context-copy", false),
        ("desktop-context-cut", true),
    ] {
        action(&compositor, format!("{command} {}", menu["generation"]));
        let clipboard = query(&compositor, "desktop-files-clipboard-read");
        assert_eq!(clipboard["cut"], cut);
        assert_eq!(
            clipboard["uris"],
            json!(["file:///tmp/context-a", "file:///tmp/context-b"])
        );
    }
    let surface = map_menu(&mut client, &menu);
    assert_eq!(client.focus(), Some(surface));
    close(&compositor, &menu);
    assert_eq!(
        client.focus(),
        Some(items[1]),
        "return to the right-clicked item, not sorted selection[0]"
    );
    assert!(context(&compositor).is_null());
    client.destroy(surface);
    assert_eq!(client.focus(), Some(items[1]));

    right_click(&compositor, point(&compositor, "file:61"));
    let menu = context(&compositor);
    let surface = map_menu(&mut client, &menu);
    click(&compositor, 650.0, 330.0);
    assert!(context(&compositor).is_null());
    assert_eq!(client.focus(), Some(normal));
    assert_eq!(client.presses().last().unwrap().0, normal);
    assert_eq!(client.releases().last().unwrap().0, normal);
    client.destroy(surface);
    assert_eq!(
        client.focus(),
        Some(normal),
        "late menu teardown must retain outside focus"
    );
    finish(&mut compositor);
}

#[test]
fn desktop_context_keyboard_escape_rejects_late_and_stale_menu_surfaces() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, _, items) = fixture();
    action(&compositor, "desktop-select \"file:62\"");
    keys(&compositor, &[42, 68]); // Shift+F10.
    let first = context(&compositor);
    assert_eq!(first["ids"], json!(["file:62"]));
    keys(&compositor, &[1]); // Cancel before the asynchronous shell maps its menu.
    assert!(context(&compositor).is_null());
    let late = map_menu(&mut client, &first);
    assert!(client.closed().contains(&late));
    assert_eq!(client.focus(), Some(items[1]));
    assert!(
        query(&compositor, "status")["shell_panels"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    keys(&compositor, &[42, 68]);
    let current = context(&compositor);
    assert_ne!(current["generation"], first["generation"]);
    let surface = map_menu(&mut client, &current);
    assert_eq!(client.focus(), Some(surface));
    let stale = map_menu(&mut client, &first);
    assert!(client.closed().contains(&stale));
    assert_eq!(
        client.focus(),
        Some(surface),
        "a stale generation cannot steal focus"
    );
    close(&compositor, &first);
    assert_eq!(context(&compositor), current);
    for command in [
        "desktop-context-read",
        "desktop-context-parent",
        "desktop-context-select-all",
        "desktop-context-arrange",
        "desktop-context-copy",
        "desktop-context-cut",
    ] {
        assert!(
            compositor
                .send_text_command(&format!("{command} {}", first["generation"]))
                .starts_with("err ")
        );
    }
    assert_eq!(
        query(
            &compositor,
            &format!("desktop-context-read {}", current["generation"])
        )["context"],
        current
    );
    close(&compositor, &current);
    client.destroy(surface);
    assert_eq!(client.focus(), Some(items[1]));
    finish(&mut compositor);
}

fn app_icon_teardown_before_close(unmap_first: bool) {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, _, _) = fixture();
    let icon = client.add(
        hearthspace_common::desktop_item_app_id("app:test.desktop"),
        (100, 108),
        (0, 0),
        [0, 255, 0, 255],
    );
    action(
        &compositor,
        "desktop-register {\"offset\":0,\"ids\":[\"file:61\",\"file:62\",\"app:test.desktop\"]}",
    );
    right_click(&compositor, point(&compositor, "app:test.desktop"));
    let menu = context(&compositor);
    assert_eq!(menu["ids"], json!(["app:test.desktop"]));
    let surface = map_menu(&mut client, &menu);
    assert_eq!(client.focus(), Some(surface));
    if unmap_first {
        client.unmap(surface);
    } else {
        client.destroy(surface);
    }
    assert_eq!(client.focus(), Some(icon));
    assert!(context(&compositor).is_null());
    // A saved graphics preference can delay close IPC behind the worker.
    close(&compositor, &menu);
    assert_eq!(client.focus(), Some(icon));
    if unmap_first {
        client.destroy(surface);
        assert_eq!(client.focus(), Some(icon));
    }
    finish(&mut compositor);
}

#[test]
fn desktop_context_destroy_before_close_restores_app_icon() {
    app_icon_teardown_before_close(false);
}

#[test]
fn desktop_context_unmap_before_close_restores_app_icon() {
    app_icon_teardown_before_close(true);
}

#[test]
fn desktop_context_canvas_menu_clamps_to_scaled_output_and_ignores_canvas_pan_zoom() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_options("800x600", None, None, 2);
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    right_click(&compositor, (398.0, 298.0));
    let menu = context(&compositor);
    assert_eq!(menu["ids"], json!([]));
    assert_eq!(menu["max_width"], 384);
    assert_eq!(menu["max_height"], 236);
    let surface = map_menu(&mut client, &menu);
    assert_eq!(client.focus(), Some(surface));
    let before = panel(&compositor);
    assert_eq!(
        (before["x"].as_i64(), before["y"].as_i64()),
        (Some(182), Some(122))
    );
    assert_eq!(
        (before["width"].as_i64(), before["height"].as_i64()),
        (Some(210), Some(170))
    );
    click(&compositor, 202.0, 142.0);
    assert_eq!(client.presses().last(), Some(&(surface, (28.0, 26.0))));
    assert_eq!(context(&compositor), menu);
    action(&compositor, "pan-to -230 170");
    action(&compositor, "zoom-out");
    assert_eq!(panel(&compositor), before);
    close(&compositor, &menu);
    assert_eq!(client.focus(), None);
    client.destroy(surface);
    assert_eq!(client.focus(), None);
    finish(&mut compositor);
}

#[test]
fn desktop_context_lock_cancels_pending_menu_without_stealing_a_later_app_release() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, normal, _) = fixture();
    let p = point(&compositor, "file:61");
    action(&compositor, format!("pointer-motion-abs {} {}", p.0, p.1));
    action(&compositor, "pointer-button-down 273");
    let pending = context(&compositor);
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = super::session_lock::lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    // Lock recovery drains the original input sequence; the next physical
    // right click belongs entirely to the application.
    locker.unlock();
    assert!(context(&compositor).is_null());
    let stale = map_menu(&mut client, &pending);
    assert!(client.closed().contains(&stale));
    let presses = client.presses().len();
    let releases = client.releases().len();
    right_click(&compositor, (650.0, 330.0));
    assert_eq!(client.presses().len(), presses + 1);
    assert_eq!(client.releases().len(), releases + 1);
    assert_eq!(client.presses().last().unwrap().0, normal);
    assert_eq!(client.releases().last().unwrap().0, normal);
    finish(&mut compositor);
}
