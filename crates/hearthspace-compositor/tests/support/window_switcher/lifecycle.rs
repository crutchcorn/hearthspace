//! Native surface lifecycle and pointer ownership for the live switcher.
use super::*;
use crate::canvas_map_client::MapClient;
use crate::shell_panels::{click, connect};
use serde_json::json;

fn apps(client: &mut MapClient, compositor: &HeadlessCompositor) -> Vec<u64> {
    for app in ["dev.SwitcherOne", "dev.SwitcherTwo"] {
        client.add(app.into(), (500, 320), (0, 0), [0x66, 0xcc, 0, 0xff]);
    }
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].as_u64().unwrap())
        .collect()
}
fn open(compositor: &HeadlessCompositor) -> Value {
    action(compositor, "key-down 56");
    keys(compositor, &[15]);
    settled(compositor)
}
fn item_center(status: &Value, id: u64) -> (f64, f64) {
    let p = status["window_switcher"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["window_id"] == id)
        .unwrap();
    (
        p["x"].as_f64().unwrap() + p["width"].as_f64().unwrap() / 2.,
        p["y"].as_f64().unwrap() + p["height"].as_f64().unwrap() / 2.,
    )
}

#[test]
fn window_switcher_pointer_commit_cancel_and_enter_never_forward_preview_clicks() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let ids = apps(&mut client, &compositor);
    let opened = open(&compositor);
    assert_eq!(client.focus(), Some(1));
    let (x, y) = item_center(&opened, ids[0]);
    click(&compositor, x, y);
    assert_eq!(client.focus(), Some(0));
    assert!(client.presses().is_empty());
    assert!(client.releases().is_empty());
    action(&compositor, "key-up 56");
    let committed = settled(&compositor);
    assert_eq!(committed["window_switcher"]["active"], false);
    open(&compositor);
    click(&compositor, 5., 5.);
    assert_eq!(client.focus(), Some(0));
    action(&compositor, "key-up 56");
    settled(&compositor);
    open(&compositor);
    keys(&compositor, &[28]);
    assert_eq!(client.focus(), Some(1));
    action(&compositor, "key-up 56");
    assert_eq!(
        state(&compositor, false, None)["window_switcher"]["active"],
        false
    );
    assert!(client.presses().is_empty());
    assert!(client.releases().is_empty());
    let before = client.keys().len();
    keys(&compositor, &[30]);
    assert_eq!(&client.keys()[before..], &[30, 30]);
}

#[test]
fn window_switcher_caption_is_passive_session_scoped_and_measurement_is_revision_guarded() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let ids = apps(&mut client, &compositor);
    open(&compositor);
    let metadata = query(&compositor, "window-switcher-status");
    assert!(metadata.get("scene").is_none());
    assert!(metadata.get("items").is_none());
    let session = metadata["session"].as_u64().unwrap();
    let revision = metadata["revision"].as_u64().unwrap();
    action(
        &compositor,
        format!(
            "window-switcher-caption-size {}",
            json!({"session":session,"revision":revision,"width":180.})
        ),
    );
    let measured = query(&compositor, "window-switcher-status");
    assert_eq!(measured["revision"], revision);
    assert_eq!(measured["caption"]["width"], 180.);
    let caption = client.add(
        hearthspace_common::window_switcher_caption_app_id(session),
        (180, 24),
        (0, 0),
        [255, 255, 255, 255],
    );
    assert_eq!(client.focus(), Some(1));
    assert_eq!(
        query(&compositor, "windows")["windows"]
            .as_array()
            .unwrap()
            .len(),
        ids.len()
    );
    keys(&compositor, &[15]);
    assert!(
        compositor
            .send_text_command(&format!(
                "window-switcher-caption-size {}",
                json!({"session":session,"revision":revision,"width":120.})
            ))
            .starts_with("err ")
    );
    keys(&compositor, &[1]);
    action(&compositor, "key-up 56");
    assert!(client.closed().contains(&caption));
    let inactive = query(&compositor, "window-switcher-status");
    assert_eq!(inactive["active"], false);
    assert_eq!(inactive["title"], "");
    let reopened = open(&compositor);
    assert_ne!(reopened["window_switcher"]["session"], session);
    let stale = client.add(
        hearthspace_common::window_switcher_caption_app_id(session),
        (180, 24),
        (0, 0),
        [255, 255, 255, 255],
    );
    assert!(client.closed().contains(&stale));
    assert_eq!(client.focus(), Some(1));
    keys(&compositor, &[1]);
    action(&compositor, "key-up 56");
}

#[test]
fn window_switcher_from_canvas_search_restores_original_focus_and_owns_only_its_motion() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    apps(&mut client, &compositor);
    action(&compositor, "canvas-search-open");
    let search = query(&compositor, "canvas-search-status");
    assert_eq!(
        query(&compositor, "window-switcher-status")["animating"],
        false
    );
    let field = client.add(
        hearthspace_common::canvas_search_app_id(
            search["session"].as_u64().unwrap(),
            search["output"].as_str(),
        ),
        (420, 54),
        (0, 0),
        [255, 255, 255, 255],
    );
    assert_eq!(client.focus(), Some(field));
    open(&compositor);
    assert_eq!(query(&compositor, "canvas-search-status")["active"], false);
    assert_eq!(
        query(&compositor, "canvas-search-status")["scene"],
        json!([])
    );
    assert_eq!(client.focus(), Some(1));
    keys(&compositor, &[28]);
    assert_eq!(client.focus(), Some(0));
    action(&compositor, "key-up 56");
    assert!(client.closed().contains(&field));
}

#[test]
fn window_switcher_can_supersede_a_canvas_search_field_before_mapping() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    apps(&mut client, &compositor);
    action(&compositor, "canvas-search-open");
    let search = query(&compositor, "canvas-search-status");
    keys(&compositor, &[30]);
    action(&compositor, "key-down 56");
    action(&compositor, "key-down 100");
    action(&compositor, "key-up 100");
    keys(&compositor, &[15]);
    assert_eq!(settled(&compositor)["window_switcher"]["active"], true);
    assert_eq!(query(&compositor, "canvas-search-status")["active"], false);
    assert_eq!(client.focus(), Some(1));
    let late = client.add(
        hearthspace_common::canvas_search_app_id(
            search["session"].as_u64().unwrap(),
            search["output"].as_str(),
        ),
        (420, 54),
        (0, 0),
        [255, 255, 255, 255],
    );
    assert!(client.closed().contains(&late));
    assert!(
        !client.keys().contains(&30),
        "cancelled search text leaked to a client"
    );
    keys(&compositor, &[1]);
    action(&compositor, "key-up 56");
    assert_eq!(client.focus(), Some(1));
}
