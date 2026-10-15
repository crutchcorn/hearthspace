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
    assert_eq!(status["recovery_ready"], true);
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
    // Releasing this physical key while asleep/inactive would not reach the
    // compositor. Lock preparation must clear it before the locker takes focus.
    action(&compositor, "key-down 42");
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
        .ends_with('b')
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
fn sleep_shield_survives_locker_launch_failure_and_admits_only_its_first_owner() {
    use super::interactive::client::InteractiveClient;

    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let empty_path = tempfile::tempdir().unwrap();
    let mut compositor =
        HeadlessCompositor::spawn_with_layout_and_path("1280x800", None, Some(empty_path.path()));
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id(),));
    let mut client = InteractiveClient::connect(&socket);
    let windows = wait_windows(&compositor, |windows| windows.len() == 2);
    let window = &windows[1];
    action(
        &compositor,
        format!(
            "pointer-motion-abs {} {}",
            window["x"].as_f64().unwrap() + 30.0,
            window["y"].as_f64().unwrap() + 30.0,
        ),
    );
    action(&compositor, "pointer-button-down 272");
    assert!(client.released_buttons().is_empty());
    let response = compositor.send_text_command("lock-shield");
    assert!(
        response.starts_with("err failed to launch swaylock"),
        "{response}"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while query(&compositor, "status")["lock_confirmed"] != true {
        assert!(Instant::now() < deadline, "opaque shield did not present");
        thread::sleep(Duration::from_millis(20));
    }
    assert_locked(&compositor);
    assert_eq!(
        client.released_buttons(),
        &[272],
        "held button must release before isolation"
    );

    let mut owner = lock_client::LockClient::connect(&socket);
    owner.wait_locked();
    let mut competing = lock_client::LockClient::connect(&socket);
    competing.wait_rejected();
    drop(competing);
    action(&compositor, "lock-shield");
    assert_locked(&compositor);
    owner.unlock();
    wait_windows(&compositor, |windows| windows.len() == 2);
    assert!(compositor.take_screenshot().starts_with(b"\x89PNG"));

    assert!(
        compositor
            .send_text_command("lock-shield")
            .starts_with("err failed to launch swaylock")
    );
    let mut owner = lock_client::LockClient::connect(&socket);
    owner.wait_locked();
    drop(owner);
    let mut replacement = lock_client::LockClient::connect(&socket);
    replacement.wait_rejected();
    assert_locked(&compositor);
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

#[test]
fn authentication_before_sleep_never_unlocks_the_resumed_desktop() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let empty_path = tempfile::tempdir().unwrap();
    let mut compositor =
        HeadlessCompositor::spawn_with_layout_and_path("1280x800", None, Some(empty_path.path()));
    drop(compositor.wait_for_socket());
    let (_editor, _) = editor(&compositor);
    let before = wait_windows(&compositor, |windows| windows.len() == 1);
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id(),));
    let mut owner = lock_client::LockClient::connect(&socket);
    owner.wait_locked();
    // Sleep preparation must latch an already established lock as well.
    action(&compositor, "prepare-sleep");
    owner.unlock();
    assert_locked(&compositor);
    // Missing replacement authentication UI is an error, never an unlock.
    assert!(
        compositor
            .send_text_command("session-resume")
            .starts_with("err failed to launch swaylock")
    );
    assert_locked(&compositor);
    let mut owner = lock_client::LockClient::connect(&socket);
    owner.wait_locked();
    assert_locked(&compositor);
    owner.unlock();
    wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["id"] == before[0]["id"] && windows[0]["focused"] == true
    });
    assert_eq!(query(&compositor, "status")["locked"], false);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn resume_preserves_live_locker_and_never_replaces_a_crashed_sleep_owner() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let empty_path = tempfile::tempdir().unwrap();
    let mut compositor =
        HeadlessCompositor::spawn_with_layout_and_path("800x600", None, Some(empty_path.path()));
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id(),));
    let mut owner = lock_client::LockClient::connect(&socket);
    owner.wait_locked();
    action(&compositor, "prepare-sleep");
    action(&compositor, "session-resume");
    assert_locked(&compositor);
    owner.unlock();
    assert_eq!(query(&compositor, "status")["locked"], false);

    // A launch error still arms the latch before the first owner arrives.
    assert!(
        compositor
            .send_text_command("prepare-sleep")
            .starts_with("err failed to launch swaylock")
    );
    let mut owner = lock_client::LockClient::connect(&socket);
    owner.wait_locked();
    drop(owner);
    action(&compositor, "session-resume");
    let mut replacement = lock_client::LockClient::connect(&socket);
    replacement.wait_rejected();
    assert_locked(&compositor);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
