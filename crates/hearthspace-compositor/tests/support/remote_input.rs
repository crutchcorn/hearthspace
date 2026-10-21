//! Exercise privileged input through Wayland, without command-socket injection.
use super::{
    navigation::{Editors, action, keys, query, wait_windows},
    *,
};
use serde_json::Value;

#[path = "remote_input_client.rs"]
mod client;
use client::RemoteClient;

fn socket(compositor: &HeadlessCompositor) -> PathBuf {
    compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()))
}

fn keymap() -> String {
    r#"xkb_keymap {
        xkb_keycodes { include "evdev+aliases(qwertz)" };
        xkb_types { include "complete" };
        xkb_compatibility { include "complete" };
        xkb_symbols { include "pc+de+inet(evdev)" };
    };"#
    .into()
}

fn editor(compositor: &HeadlessCompositor) -> (Editors, PathBuf) {
    let input = compositor.runtime_dir.path().join("remote-editor.txt");
    let output = compositor.runtime_dir.path().join("remote-editor.json");
    std::fs::write(&input, "").unwrap();
    let log =
        std::fs::File::create(compositor.runtime_dir.path().join("remote-editor.log")).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_hearthspace-clipboard-test"))
        .args([
            "--title",
            "Remote keyboard editor",
            "--input-file",
            input.to_str().unwrap(),
            "--output-file",
            output.to_str().unwrap(),
        ])
        .env("XDG_RUNTIME_DIR", compositor.runtime_dir.path())
        .env("WAYLAND_DISPLAY", socket(compositor))
        .env("GDK_BACKEND", "wayland")
        .env_remove("DISPLAY")
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
    wait_windows(compositor, |windows| {
        windows.len() == 1 && windows[0]["focused"] == true
    });
    wait_text(&output, "");
    (Editors(vec![child]), output)
}

fn wait_text(path: &Path, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let observed = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        if observed
            .as_ref()
            .is_some_and(|state| state["text"] == expected)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "expected editor text {expected:?}, observed {observed:?}"
        );
        thread::sleep(Duration::from_millis(15));
    }
}

#[test]
fn remote_input_globals_require_explicit_compositor_opt_in() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn();
    drop(compositor.wait_for_socket());
    assert!(!RemoteClient::connect(&socket(&compositor)).enabled());
}

#[test]
fn remote_keyboard_keymaps_modifiers_disconnects_and_lock_preserve_local_input() {
    let _guard = headless_test_guard();
    let mut compositor =
        HeadlessCompositor::spawn_with_remote_options("1280x800", None, None, 1, true);
    drop(compositor.wait_for_socket());
    let (_editor, output) = editor(&compositor);
    let mut remote = RemoteClient::connect(&socket(&compositor));
    assert!(remote.enabled());
    remote.set_keymap(&keymap());
    remote.key(21, true); // German keymap: the physical US Y key types z.
    remote.key(21, false);
    remote.modifiers(1); // WayVNC uses masks for levels, without a Shift key.
    remote.key(21, true);
    remote.key(21, false);
    remote.modifiers(0);
    keys(&compositor, &[21]); // Physical typing restores the configured US map.
    wait_text(&output, "zZy");

    action(&compositor, "key-down 42");
    remote.key(42, true);
    remote.destroy_keyboard();
    keys(&compositor, &[21]);
    action(&compositor, "key-up 42");
    wait_text(&output, "zZyY"); // Remote teardown must retain physical Shift.

    let mut remote = RemoteClient::connect(&socket(&compositor));
    remote.set_keymap(&keymap());
    remote.key(42, true);
    drop(remote); // A lost connection must release its outstanding modifiers.
    // Resource destruction is dispatched asynchronously from the socket hangup.
    // A new connection/barrier lets the server process its previous peer first.
    let _barrier = RemoteClient::connect(&socket(&compositor));
    keys(&compositor, &[21]);
    wait_text(&output, "zZyYy");

    let mut remote = RemoteClient::connect(&socket(&compositor));
    remote.set_keymap(&keymap());
    let mut locker = super::session_lock::lock_client::LockClient::connect(&socket(&compositor));
    locker.wait_locked();
    remote.modifiers(1);
    remote.key(21, true);
    remote.key(21, false);
    locker.unlock();
    wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["focused"] == true
    });
    keys(&compositor, &[21]);
    wait_text(&output, "zZyYyy");
}

#[test]
fn remote_keyboard_disconnect_preserves_active_search_previews_and_selection() {
    let _guard = headless_test_guard();
    let mut compositor =
        HeadlessCompositor::spawn_with_remote_options("1280x800", None, None, 1, true);
    drop(compositor.wait_for_socket());
    let mut client = super::shell_panels::connect(&compositor);
    let app = client.add("Search window".into(), (500, 320), (0, 0), [0, 128, 0, 255]);
    action(&compositor, "canvas-search-motion false");
    action(&compositor, "canvas-search-open");
    let search = query(&compositor, "canvas-search-status");
    action(
        &compositor,
        format!(
            "canvas-search-update {}",
            serde_json::json!({
                "session": search["session"], "query":"window", "handwriting":false,
            })
        ),
    );
    let search = query(&compositor, "canvas-search-status");
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
    let before = query(&compositor, "canvas-search-status");
    assert_eq!(before["result_count"], 1);
    assert_eq!(before["scene"].as_array().unwrap().len(), 1);
    let mut remote = RemoteClient::connect(&socket(&compositor));
    remote.set_keymap(&keymap());
    remote.key(42, true);
    remote.destroy_keyboard(); // Teardown resets its held switcher chord too.
    let after = query(&compositor, "canvas-search-status");
    assert_eq!(after["active"], true);
    assert_eq!(after["session"], before["session"]);
    assert_eq!(after["query"], "window");
    assert_eq!(
        after["scene"], before["scene"],
        "unrelated switcher reset erased search poses"
    );
    assert_eq!(client.focus(), Some(field));

    // The preserved pose must still participate in compositor hit testing.
    let bounds = &after["scene"][0]["display"];
    let x = bounds["x"].as_f64().unwrap() + bounds["width"].as_f64().unwrap() / 2.0;
    let y = bounds["y"].as_f64().unwrap() + bounds["height"].as_f64().unwrap() / 2.0;
    super::shell_panels::click(&compositor, x, y);
    assert_eq!(query(&compositor, "canvas-search-status")["active"], false);
    assert_eq!(client.focus(), Some(app));
}

#[test]
fn remote_pointer_maps_scaled_output_and_releases_drag_on_disconnect() {
    use super::interactive::client::InteractiveClient;
    let _guard = headless_test_guard();
    let mut compositor =
        HeadlessCompositor::spawn_with_remote_options("2560x1600", None, None, 2, true);
    drop(compositor.wait_for_socket());
    let mut windows_client = InteractiveClient::connect(&socket(&compositor));
    let windows = wait_windows(&compositor, |windows| windows.len() == 2);
    let window = windows
        .iter()
        .find(|window| window["title"] == "Interactive fixture 0")
        .unwrap();
    let id = window["id"].as_u64().unwrap();
    action(&compositor, format!("move-window {id} 100 150"));
    let mut remote = RemoteClient::connect(&socket(&compositor));
    remote.motion(260, 360, 2560, 1600);
    remote.button(true);
    let serial = windows_client.press_serial();
    windows_client.request_move(0, serial);
    remote.motion(420, 460, 2560, 1600);
    wait_windows(&compositor, |windows| {
        windows.iter().any(|window| {
            window["id"] == id && window["canvas_x"] == 180 && window["canvas_y"] == 200
        })
    });
    drop(remote);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !windows_client.released_buttons().contains(&0x110) {
        assert!(
            Instant::now() < deadline,
            "remote disconnect left pointer grab held"
        );
        thread::sleep(Duration::from_millis(10));
    }
    action(&compositor, "pointer-motion-abs 600 600");
    let windows = query(&compositor, "windows");
    let window = windows["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["id"] == id)
        .unwrap();
    assert_eq!(
        (window["canvas_x"].clone(), window["canvas_y"].clone()),
        (Value::from(180), Value::from(200))
    );
}
