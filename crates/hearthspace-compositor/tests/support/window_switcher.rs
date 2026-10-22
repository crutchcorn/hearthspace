//! Live canvas previews preserve client focus and canonical placement until commit.
use super::navigation::{Editors, action, keys, query, wait_windows};
use super::navigation_client::{editor, wait_state};
use super::session_lock::lock_client;
use super::*;
use serde_json::Value;

#[path = "window_switcher/lifecycle.rs"]
mod lifecycle;

fn state(compositor: &HeadlessCompositor, active: bool, selected: Option<u64>) -> Value {
    let status = query(compositor, "status");
    assert_eq!(status["window_switcher"]["active"], active, "{status}");
    if let Some(id) = selected {
        assert_eq!(
            status["window_switcher"]["selected_window_id"], id,
            "{status}"
        );
    }
    status
}

fn settled(compositor: &HeadlessCompositor) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = query(compositor, "status");
        if status["viewport"]["animating"] == false
            && status["window_switcher"]["animating"] == false
        {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "viewport never settled: {status}"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn offset(status: &Value) -> (i64, i64) {
    (
        status["viewport"]["x"].as_i64().unwrap(),
        status["viewport"]["y"].as_i64().unwrap(),
    )
}

#[test]
fn held_alt_tab_keeps_preview_modal_until_release_and_preserves_modifier_pairing() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let (first, first_state) = editor(&compositor, &mut editors, 0);
    let (second, second_state) = editor(&compositor, &mut editors, 1);
    let (third, third_state) = editor(&compositor, &mut editors, 2);
    let before = compositor.take_screenshot();

    action(&compositor, "key-down 56");
    keys(&compositor, &[15]);
    let opened = state(&compositor, true, Some(second));
    assert_focus(&opened, third);
    assert_eq!(
        opened["window_switcher"]["window_ids"],
        serde_json::json!([third, second, first])
    );
    assert_eq!(
        opened["window_switcher"]["items"].as_array().unwrap().len(),
        3
    );
    assert_ne!(
        compositor.take_screenshot(),
        before,
        "window list never rendered"
    );
    keys(&compositor, &[15]);
    state(&compositor, true, Some(first));
    keys(&compositor, &[42, 15]);
    state(&compositor, true, Some(second));
    action(&compositor, "key-down 30"); // Kept held across overlay dismissal.
    action(&compositor, "axis 0 -120");
    assert_focus(&state(&compositor, true, Some(second)), third);
    action(&compositor, "key-up 56");
    state(&compositor, false, None);
    action(&compositor, "key-up 30");
    keys(&compositor, &[48]);
    wait_state(&second_state, |value| value["text"] == "b");
    assert_eq!(wait_state(&first_state, |v| v["ready"] == true)["text"], "");
    assert_eq!(wait_state(&third_state, |v| v["ready"] == true)["text"], "");

    action(&compositor, "key-down 56");
    keys(&compositor, &[15]);
    action(&compositor, "key-down 42");
    action(&compositor, "key-up 56");
    state(&compositor, false, None);
    action(&compositor, "key-up 42");
    keys(&compositor, &[32]);
    wait_state(&third_state, |value| value["text"] == "d");
    action(&compositor, format!("focus-window {first}"));

    action(&compositor, "key-down 56");
    keys(&compositor, &[15]);
    assert_focus(&state(&compositor, true, Some(third)), first);
    action(&compositor, "key-down 42");
    keys(&compositor, &[1]); // Escape restores the original client.
    state(&compositor, false, None);
    action(&compositor, "key-up 42");
    action(&compositor, "key-up 56");
    keys(&compositor, &[46]);
    wait_state(&first_state, |value| value["text"] == "c");
    wait_windows(&compositor, |windows| {
        windows
            .iter()
            .any(|w| w["id"] == first && w["focused"] == true)
    });
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

fn assert_focus(status: &Value, id: u64) {
    assert!(
        status["windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["id"] == id && w["focused"] == true),
        "unexpected client focus: {status}"
    );
}

fn canonical(status: &Value) -> Vec<Value> {
    let mut windows: Vec<_> = status["windows"].as_array().unwrap().iter().map(|w| serde_json::json!({"id":w["id"],"x":w["canvas_x"],"y":w["canvas_y"],"w":w["content_width"],"h":w["content_height"]})).collect();
    windows.sort_by_key(|w| w["id"].as_u64());
    windows
}

#[test]
fn window_switcher_animates_live_buffers_without_moving_focus_view_or_canonical_geometry() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let (first, _) = editor(&compositor, &mut editors, 0);
    let (second, _) = editor(&compositor, &mut editors, 1);
    let (third, _) = editor(&compositor, &mut editors, 2);
    action(&compositor, format!("move-window {first} 9000 -7000"));
    action(&compositor, format!("move-window {second} -9000 7000"));
    action(&compositor, format!("focus-window {third}"));
    let before = settled(&compositor);
    action(&compositor, "key-down 56");
    keys(&compositor, &[15]);
    let opened = state(&compositor, true, Some(second));
    assert_eq!(offset(&opened), offset(&before));
    assert_eq!(canonical(&opened), canonical(&before));
    assert_focus(&opened, third);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let current = query(&compositor, "status");
        assert_eq!(current["viewport"], before["viewport"]);
        assert_focus(&current, third);
        if current["window_switcher"]["animating"] == true
            && current["window_switcher"]["scene"]
                .as_array()
                .unwrap()
                .iter()
                .any(|pose| {
                    pose["display"] != pose["canonical"]
                        && current["window_switcher"]["items"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|item| {
                                item["window_id"] == pose["window_id"]
                                    && pose["display"] != item["target"]
                            })
                })
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "no intermediate live preview: {current}"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let grid = settled(&compositor);
    keys(&compositor, &[15]);
    let cycled = state(&compositor, true, Some(first));
    assert_eq!(
        cycled["window_switcher"]["items"],
        grid["window_switcher"]["items"]
    );
    assert_eq!(cycled["window_switcher"]["animating"], false);
    keys(&compositor, &[1]);
    action(&compositor, "key-up 56");
    let cancelled = settled(&compositor);
    assert_eq!(cancelled["viewport"], before["viewport"]);
    assert_eq!(canonical(&cancelled), canonical(&before));
    assert_focus(&cancelled, third);

    keys(&compositor, &[56, 15]); // A quick chord still reveals the chosen window.
    let completed = settled(&compositor);
    assert_focus(&completed, second);
    assert_ne!(offset(&completed), offset(&before));
    assert_eq!(canonical(&completed), canonical(&before));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn switcher_reconciles_closed_selection_and_lock_hides_all_switcher_state() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("800x600");
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let (first, _) = editor(&compositor, &mut editors, 0);
    let (second, second_state) = editor(&compositor, &mut editors, 1);
    action(&compositor, "key-down 56");
    keys(&compositor, &[15]);
    state(&compositor, true, Some(first));
    editors.0[0].kill().unwrap();
    editors.0[0].wait().unwrap();
    wait_windows(&compositor, |windows| windows.len() == 1);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = query(&compositor, "status");
        if status["window_switcher"]["selected_window_id"] == second
            && status["window_switcher"]["items"].as_array().unwrap().len() == 1
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "switcher retained a closed client: {status}"
        );
        thread::sleep(Duration::from_millis(20));
    }
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    let locked = query(&compositor, "status");
    assert_eq!(locked["locked"], true);
    assert!(locked.get("window_switcher").is_none());
    assert!(!locked.to_string().contains("Navigation editor"));
    assert!(locked["windows"].as_array().unwrap().is_empty());
    locker.unlock();
    state(&compositor, false, None);
    action(&compositor, "key-up 56");
    keys(&compositor, &[48]);
    wait_state(&second_state, |value| value["text"] == "b");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
