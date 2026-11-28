//! The first desktop drag must not depend on manually choosing a tool.
use super::*;
use crate::{session_lock::lock_client::LockClient, shell_panels};

const ITEM: &str = "file:696e697469616c";

fn desktop_item(compositor: &HeadlessCompositor) -> crate::canvas_map_client::MapClient {
    let mut client = shell_panels::connect(compositor);
    let app_id = hearthspace_common::desktop_item_app_id(ITEM);
    client.add(app_id, (100, 108), (0, 0), [40, 180, 100, 255]);
    client
}

fn item(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "desktop-status")["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == ITEM)
        .unwrap()
        .clone()
}

fn first_desktop_drag(compositor: &HeadlessCompositor) {
    assert_eq!(tool(compositor), "select");
    let before = item(compositor);
    let x = before["x"].as_f64().unwrap();
    let y = before["y"].as_f64().unwrap();
    assert_eq!(before["selected"], false);
    // Start above the item, clear of the edge-pan margin on its left.
    start(compositor, (x + 8., y - 8.));
    motion(compositor, (x + 108., y + 116.));
    finish(compositor);
    let selected = item(compositor);
    assert_eq!(selected["selected"], true, "first marquee was inert");
    start(compositor, (x + 40., y + 40.));
    motion(compositor, (x + 70., y + 64.));
    finish(compositor);
    let moved = item(compositor);
    assert_eq!(moved["x"].as_f64().unwrap(), x + 30.);
    assert_eq!(moved["y"].as_f64().unwrap(), y + 24.);
    assert_eq!(offset(compositor), [0, 0]);
    assert_eq!(tool(compositor), "select");
}

#[test]
fn canvas_startup_default_tool_allows_first_desktop_drag() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let _desktop = desktop_item(&compositor);
    first_desktop_drag(&compositor);
}

#[test]
fn canvas_startup_app_maximize_keeps_default_tool_for_desktop_drag_after_restore() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut applications = clients(&compositor);
    let windows = query(&compositor, "windows");
    for window in windows["windows"].as_array().unwrap() {
        action(&compositor, format!("move-window {} 800 400", window["id"]));
    }
    let _desktop = desktop_item(&compositor);
    let id = windows["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["title"] == "Interactive fixture 1")
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    action(&compositor, format!("focus-window {id}"));
    applications.gtk_request_maximized(1, true);
    wait_windows(&compositor, |windows| {
        windows
            .iter()
            .any(|window| window["id"] == id && window["focused_view"] == true)
    });
    assert_eq!(tool(&compositor), "select");
    applications.gtk_request_maximized(1, false);
    wait_windows(&compositor, |windows| {
        windows
            .iter()
            .any(|window| window["id"] == id && window["focused_view"] == false)
    });
    first_desktop_drag(&compositor);
}

#[test]
fn canvas_startup_resume_restores_default_tool_without_reusing_held_pan() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let _desktop = desktop_item(&compositor);
    action(&compositor, "canvas-tool pan");
    start(&compositor, (600., 600.));
    action(&compositor, "session-resume");
    motion(&compositor, (650., 650.));
    finish(&compositor);
    assert_eq!(
        offset(&compositor),
        [0, 0],
        "old pan resumed after recovery"
    );
    first_desktop_drag(&compositor);
}

#[test]
fn canvas_startup_unlock_restores_default_tool_before_first_desktop_drag() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let _desktop = desktop_item(&compositor);
    action(&compositor, "canvas-tool pan");
    let mut locker = LockClient::connect(&socket(&compositor));
    locker.wait_locked();
    assert_eq!(tool(&compositor), Value::Null);
    locker.unlock();
    first_desktop_drag(&compositor);
}
