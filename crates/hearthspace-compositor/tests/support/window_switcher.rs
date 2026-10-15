//! Held switcher input, live focus, animation and recovery against real clients.
use super::navigation::{Editors, action, keys, query, wait_windows};
use super::overview::{editor, wait_state};
use super::session_lock::lock_client;
use super::*;
use serde_json::Value;

fn state(compositor: &HeadlessCompositor, active: bool, selected: Option<u64>) -> Value {
    let status = query(compositor, "status");
    assert_eq!(status["window_switcher"]["active"], active, "{status}");
    if let Some(id) = selected {
        assert_eq!(
            status["window_switcher"]["selected_window_id"], id,
            "{status}"
        );
        assert!(
            status["windows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|window| { window["id"] == id && window["focused"] == true }),
            "switcher selection does not have client focus: {status}"
        );
    }
    status
}

fn settled(compositor: &HeadlessCompositor) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = query(compositor, "status");
        if status["viewport"]["animating"] == false {
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
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let (first, first_state) = editor(&compositor, &mut editors, 0);
    let (second, second_state) = editor(&compositor, &mut editors, 1);
    let (_, third_state) = editor(&compositor, &mut editors, 2);
    let before = compositor.take_screenshot();

    action(&compositor, "key-down 56");
    keys(&compositor, &[15]);
    let opened = state(&compositor, true, Some(first));
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
    state(&compositor, true, Some(second));
    keys(&compositor, &[42, 15]);
    state(&compositor, true, Some(first));
    action(&compositor, "key-down 30"); // Kept held across overlay dismissal.
    action(&compositor, "axis 0 -120");
    state(&compositor, true, Some(first));
    action(&compositor, "key-up 56");
    state(&compositor, false, None);
    action(&compositor, "key-up 30");
    keys(&compositor, &[48]);
    wait_state(&first_state, |value| value["text"] == "b");
    assert_eq!(
        wait_state(&second_state, |v| v["ready"] == true)["text"],
        ""
    );
    assert_eq!(wait_state(&third_state, |v| v["ready"] == true)["text"], "");

    action(&compositor, "key-down 56");
    keys(&compositor, &[15]);
    action(&compositor, "key-down 42");
    action(&compositor, "key-up 56");
    state(&compositor, false, None);
    action(&compositor, "key-up 42");
    keys(&compositor, &[32]);
    wait_state(&second_state, |value| value["text"] == "d");
    action(&compositor, format!("focus-window {first}"));

    action(&compositor, "key-down 56");
    keys(&compositor, &[15]);
    state(&compositor, true, Some(second));
    action(&compositor, "key-down 42");
    keys(&compositor, &[1]); // Escape restores the original client.
    state(&compositor, false, None);
    action(&compositor, "key-up 42");
    action(&compositor, "key-up 56");
    keys(&compositor, &[46]);
    wait_state(&first_state, |value| value["text"] == "bc");
    wait_windows(&compositor, |windows| {
        windows
            .iter()
            .any(|w| w["id"] == first && w["focused"] == true)
    });
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn held_switching_animates_retargets_from_the_current_view_and_escape_restores_it() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
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
    let origin = offset(&before);
    action(&compositor, "key-down 56");
    keys(&compositor, &[15]);
    state(&compositor, true, Some(first));
    let deadline = Instant::now() + Duration::from_secs(3);
    let middle = loop {
        let current = query(&compositor, "status");
        if current["viewport"]["animating"] == true && offset(&current) != origin {
            break current;
        }
        assert!(
            Instant::now() < deadline,
            "switch snapped without an intermediate frame: {current}"
        );
        thread::sleep(Duration::from_millis(5));
    };
    keys(&compositor, &[15]); // Reverse destination while the first pan is running.
    let retarget = state(&compositor, true, Some(second));
    assert_eq!(retarget["viewport"]["animating"], true);
    assert_ne!(
        offset(&retarget),
        origin,
        "retarget reset to the pre-switch view"
    );
    assert_ne!(offset(&middle), origin);
    let destination = settled(&compositor);
    assert!(
        destination["windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["id"] == second && w["visible"] == true && w["focused"] == true)
    );
    assert_ne!(offset(&destination), origin);
    assert_eq!(
        destination["viewport"]["scale"],
        before["viewport"]["scale"]
    );
    keys(&compositor, &[1]);
    state(&compositor, false, None);
    action(&compositor, "key-up 56");
    assert_eq!(offset(&settled(&compositor)), origin);
    wait_windows(&compositor, |windows| {
        windows
            .iter()
            .any(|w| w["id"] == third && w["focused"] == true)
    });

    keys(&compositor, &[56, 15]); // A quick chord still finishes its pan after release.
    state(&compositor, false, None);
    let completed = settled(&compositor);
    assert!(
        completed["windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["id"] == first && w["visible"] == true && w["focused"] == true)
    );
    assert!(
        completed["windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["id"] == first && w["canvas_x"] == 9000 && w["canvas_y"] == -7000)
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn switcher_reconciles_closed_selection_and_lock_hides_all_switcher_state() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
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
    assert!(!locked.to_string().contains("Overview editor"));
    assert!(locked["windows"].as_array().unwrap().is_empty());
    locker.unlock();
    state(&compositor, false, None);
    action(&compositor, "key-up 56");
    keys(&compositor, &[48]);
    wait_state(&second_state, |value| value["text"] == "b");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
