use super::navigation::{Editors, action, keys, query, wait_windows};
use super::*;
use serde_json::Value;

#[path = "lock_client.rs"]
mod lock_client;

fn editor(compositor: &HeadlessCompositor) -> (Editors, PathBuf) {
    let input = compositor.runtime_dir.path().join("private-editor.txt");
    let state = compositor.runtime_dir.path().join("private-editor.json");
    std::fs::write(&input, "keep-private").unwrap();
    let log =
        std::fs::File::create(compositor.runtime_dir.path().join("private-editor.log")).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_hearthspace-clipboard-test"))
        .args([
            "--title",
            "Private lock editor",
            "--input-file",
            input.to_str().unwrap(),
            "--output-file",
            state.to_str().unwrap(),
        ])
        .env("XDG_RUNTIME_DIR", compositor.runtime_dir.path())
        .env(
            "WAYLAND_DISPLAY",
            format!("hearthspace-test-{}", std::process::id()),
        )
        .env("GDK_BACKEND", "wayland")
        .env_remove("DISPLAY")
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
    (Editors(vec![child]), state)
}

fn snapshot(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn assert_locked(compositor: &HeadlessCompositor) {
    let status = query(compositor, "status");
    assert_eq!(status["locked"], true);
    assert_eq!(status["lock_confirmed"], true);
    assert!(status["windows"].as_array().unwrap().is_empty());
    assert!(!status.to_string().contains("Private lock editor"));
    for command in [
        "windows",
        "apps",
        "capture",
        "screenshot",
        "key-down 30",
        "key-up 30",
        "pointer-motion-abs 200 200",
        "pointer-button-down 272",
        "pointer-button-up 272",
        "axis 0 120",
        "clipboard-set 68656c6c6f",
        "focus-next-window",
        "spawn foot",
        "log-a11y-tree",
        "pan-left",
    ] {
        assert!(
            compositor
                .send_text_command(command)
                .starts_with("err session is locked"),
            "{command}"
        );
    }
}

#[test]
fn session_lock_blocks_agent_access_and_restores_focus_only_after_owner_unlocks() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let (_editor, state_path) = editor(&compositor);
    let windows = wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["focused"] == true
    });
    let id = windows[0]["id"].clone();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !state_path.exists() {
        assert!(
            Instant::now() < deadline,
            "editor did not write its initial state"
        );
        thread::sleep(Duration::from_millis(25));
    }
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    assert_locked(&compositor);
    assert_eq!(snapshot(&state_path)["text"], "keep-private");
    let mut competing = lock_client::LockClient::connect(&socket);
    competing.wait_rejected();
    drop(competing);
    assert_locked(&compositor);
    locker.destroy_surfaces();
    assert_locked(&compositor);
    locker.unlock();
    wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["id"] == id && windows[0]["focused"] == true
    });
    keys(&compositor, &[48]);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !snapshot(&state_path)["text"]
        .as_str()
        .unwrap()
        .contains('b')
    {
        assert!(
            Instant::now() < deadline,
            "editor did not regain real input after unlock"
        );
        thread::sleep(Duration::from_millis(25));
    }
    assert!(compositor.take_screenshot().starts_with(b"\x89PNG"));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn session_lock_client_disconnect_keeps_the_session_closed() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_size("800x600");
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    drop(locker);
    let mut replacement = lock_client::LockClient::connect(&socket);
    replacement.wait_rejected();
    assert_locked(&compositor);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
