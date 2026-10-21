//! Real Wayland clients exercise temporary search poses and modal input ownership.
use super::canvas_map_client::MapClient;
use super::navigation::{action, keys, query};
use super::shell_panels::{click, connect};
use super::*;
use serde_json::{Value, json};

#[path = "canvas_search/lifecycle.rs"]
mod lifecycle;

const GREEN: [u8; 4] = [0x66, 0xcc, 0, 0xff];

fn search(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "canvas-search-status")
}

fn update(compositor: &HeadlessCompositor, session: u64, text: &str) {
    action(
        compositor,
        format!(
            "canvas-search-update {}",
            json!({"session":session,"query":text,"handwriting":false})
        ),
    );
}

fn wait_search(compositor: &HeadlessCompositor, condition: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = search(compositor);
        if condition(&state) {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "canvas search timed out: {state}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn settled(compositor: &HeadlessCompositor) -> Value {
    let mut previous = search(compositor);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        thread::sleep(Duration::from_millis(20));
        let current = search(compositor);
        if previous["scene"] == current["scene"] {
            return current;
        }
        assert!(
            Instant::now() < deadline,
            "search motion did not settle: {current}"
        );
        previous = current;
    }
}

fn result_ids(state: &Value) -> Vec<String> {
    state["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|result| result["id"].as_str().unwrap().to_owned())
        .collect()
}

fn pose<'a>(state: &'a Value, id: &str) -> &'a Value {
    state["scene"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id)
        .unwrap_or_else(|| panic!("missing pose {id}: {state}"))
}

fn canonical(compositor: &HeadlessCompositor) -> Value {
    let status = query(compositor, "status");
    let mut windows: Vec<_> = status["windows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|window| {
            json!({"id":window["id"],"x":window["canvas_x"],"y":window["canvas_y"],
                "width":window["content_width"],"height":window["content_height"]})
        })
        .collect();
    windows.sort_by_key(|window| window["id"].as_u64().unwrap());
    json!({"windows":windows,"viewport":status["viewport"],
        "desktop":query(compositor,"desktop-status")["items"]})
}

fn normal(client: &mut MapClient, compositor: &HeadlessCompositor, app: &str) -> (usize, u64) {
    let number = client.add(app.into(), (500, 320), (0, 0), GREEN);
    let windows = query(compositor, "windows");
    let id = windows["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["app_id"] == app)
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    (number, id)
}

fn field(client: &mut MapClient, state: &Value) -> usize {
    client.add(
        hearthspace_common::canvas_search_app_id(
            state["session"].as_u64().unwrap(),
            state["output"].as_str(),
        ),
        (
            state["field"]["width"].as_f64().unwrap() as i32,
            state["field"]["height"].as_f64().unwrap() as i32,
        ),
        (0, 0),
        GREEN,
    )
}

#[test]
fn canvas_search_shortcut_replays_early_typing_into_its_field_and_restores_focus() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let (normal, _) = normal(&mut client, &compositor, "dev.hearthspace.SearchTyping");
    keys(&compositor, &[29, 37]); // Ctrl+K follows the physical key policy.
    let opened = search(&compositor);
    assert_eq!(opened["active"], true);
    keys(&compositor, &[30]); // 'a' arrives before the shell maps the field.
    let field = field(&mut client, &opened);
    assert_eq!(client.focus(), Some(field));
    assert_eq!(client.keys().iter().filter(|key| **key == 30).count(), 2);
    assert!(
        client.enter_keys().is_empty(),
        "opening shortcut must not remain held"
    );
    keys(&compositor, &[1]);
    assert_eq!(search(&compositor)["active"], false);
    assert!(client.closed().contains(&field));
    assert_eq!(client.focus(), Some(normal));
    let before = client.keys().len();
    keys(&compositor, &[48]);
    assert_eq!(&client.keys()[before..], &[48, 48]);
}

#[test]
fn canvas_search_matches_all_words_orders_sources_and_resets_keyboard_selection() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let (_, notes) = normal(&mut client, &compositor, "dev.hearthspace.ProjectNotes");
    let (_, browser) = normal(&mut client, &compositor, "dev.hearthspace.ProjectBrowser");
    let desktop = ["app:notes.desktop", "file:project"];
    action(
        &compositor,
        format!("desktop-register {}", json!({"offset":0,"ids":desktop})),
    );
    for id in desktop {
        client.add(
            hearthspace_common::desktop_item_app_id(id),
            (100, 108),
            (0, 0),
            GREEN,
        );
    }
    action(
        &compositor,
        format!(
            "canvas-search-desktop-register {}",
            json!({"offset":0,"entries":[
                {"id":desktop[0],"title":"Notes","search_text":"project notes"},
                {"id":desktop[1],"title":"Project folder","search_text":"project directory"}
            ]})
        ),
    );
    action(
        &compositor,
        format!(
            "canvas-search-register {}",
            json!({"offset":0,"entries":[
                {"id":"control:sound","title":"Sound","search_text":"project volume"},
                {"id":"tool:map","title":"Map","search_text":"project canvas"}
            ]})
        ),
    );
    action(&compositor, "canvas-search-open");
    let opened = search(&compositor);
    field(&mut client, &opened);
    let session = opened["session"].as_u64().unwrap();
    assert_eq!(
        result_ids(&opened),
        [desktop[0], desktop[1], "control:sound", "tool:map"]
    );
    update(&compositor, session, "project");
    let expected = vec![
        desktop[0].to_owned(),
        desktop[1].to_owned(),
        format!("window:{browser}"),
        format!("window:{notes}"),
        "control:sound".into(),
        "tool:map".into(),
    ];
    assert_eq!(result_ids(&search(&compositor)), expected);
    // The real editor consumes Up after its IME guard, then sends this command.
    action(
        &compositor,
        format!(
            "canvas-search-step {}",
            json!({"session":session,"step":-1})
        ),
    );
    assert_eq!(search(&compositor)["selected_id"], "tool:map");
    update(&compositor, session, "PROJECT "); // Same matches, new query resets selection.
    assert_eq!(search(&compositor)["selected_id"], desktop[0]);
    update(&compositor, session, "PrOjEcT nOtEs");
    assert_eq!(
        result_ids(&search(&compositor)),
        [desktop[0].to_owned(), format!("window:{notes}")]
    );
    update(&compositor, session, "there-is-no-match");
    let empty = search(&compositor);
    assert_eq!(empty["result_count"], 0);
    assert_eq!(empty["selected_id"], Value::Null);
    assert_eq!(empty["field"]["height"], 96.0);
    assert!(
        compositor
            .send_text_command(&format!(
                "canvas-search-activate {}",
                json!({"session":session})
            ))
            .starts_with("err ")
    );
    assert_eq!(
        search(&compositor)["active"],
        true,
        "Enter with no match stays open"
    );
}

#[test]
fn canvas_search_animates_live_previews_without_changing_canonical_layout() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let (_, notes) = normal(&mut client, &compositor, "dev.hearthspace.MotionNotes");
    let (browser, browser_id) = normal(&mut client, &compositor, "dev.hearthspace.MotionBrowser");
    action(&compositor, format!("move-window {notes} 9000 -7000"));
    action(&compositor, format!("move-window {browser_id} 220 180"));
    let before = canonical(&compositor);
    action(&compositor, "canvas-search-open");
    let opened = search(&compositor);
    field(&mut client, &opened);
    let session = opened["session"].as_u64().unwrap();
    update(&compositor, session, "Motion");
    let during = search(&compositor);
    let id = format!("window:{notes}");
    let shown = settled(&compositor);
    assert_ne!(
        pose(&during, &id)["display"],
        pose(&shown, &id)["display"],
        "preview must animate"
    );
    assert_ne!(pose(&shown, &id)["display"], pose(&shown, &id)["canonical"]);
    assert!(pose(&shown, &id)["display"]["width"].as_f64().unwrap() < 500.0);
    assert_eq!(canonical(&compositor), before);
    update(&compositor, session, "MotionNotes");
    let filtered = settled(&compositor);
    let hidden = pose(&filtered, &format!("window:{browser_id}"));
    assert_eq!(hidden["matches"], false);
    assert_eq!(hidden["opacity"], 0.0);
    action(&compositor, "axis 0 120");
    assert_eq!(
        canonical(&compositor),
        before,
        "scroll must not pan the underlying canvas"
    );
    keys(&compositor, &[1]);
    wait_search(&compositor, |s| s["scene"].as_array().unwrap().is_empty());
    assert_eq!(canonical(&compositor), before);
    assert_eq!(client.focus(), Some(browser));
}

#[test]
fn canvas_search_window_click_reveals_existing_client_without_clicking_through() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let (notes, id) = normal(&mut client, &compositor, "dev.hearthspace.ActivateNotes");
    action(&compositor, format!("move-window {id} 9000 -7000"));
    let before = canonical(&compositor);
    action(&compositor, "canvas-search-open");
    let session = search(&compositor)["session"].as_u64().unwrap();
    update(&compositor, session, "ActivateNotes");
    let shown = settled(&compositor);
    let display = &pose(&shown, &format!("window:{id}"))["display"];
    click(
        &compositor,
        display["x"].as_f64().unwrap() + display["width"].as_f64().unwrap() / 2.0,
        display["y"].as_f64().unwrap() + display["height"].as_f64().unwrap() / 2.0,
    );
    assert_eq!(search(&compositor)["active"], false);
    assert_eq!(client.focus(), Some(notes));
    assert!(
        client.presses().is_empty(),
        "search activation must not reach application content"
    );
    assert!(client.releases().is_empty());
    wait_search(&compositor, |s| s["scene"].as_array().unwrap().is_empty());
    let after = canonical(&compositor);
    assert_eq!(after["windows"], before["windows"]);
    assert_ne!(after["viewport"], before["viewport"]);
    let windows = query(&compositor, "windows");
    let window = &windows["windows"][0];
    assert_eq!(window["id"], id);
    assert_eq!(window["visible"], true);
    let center_x = window["x"].as_f64().unwrap() + window["width"].as_f64().unwrap() / 2.0;
    let center_y = window["y"].as_f64().unwrap() + window["height"].as_f64().unwrap() / 2.0;
    assert!((center_x - 640.0).abs() <= 1.0);
    assert!((center_y - 400.0).abs() <= 1.0);
}
