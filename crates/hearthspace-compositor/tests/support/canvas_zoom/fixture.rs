//! Client maximize starts in focused view; canvas tests opt in through Super+M.
use super::*;
use serde_json::Value;

pub(super) fn maximized_fixture(
    compositor: &HeadlessCompositor,
    name: &str,
    backend: &str,
) -> (Editors, PathBuf) {
    let existing_ids: Vec<_> = query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|window| window["id"].as_u64().unwrap())
        .collect();
    let path = compositor.runtime_dir.path().join(format!("{name}.json"));
    let log = std::fs::File::create(path.with_extension("log")).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_hearthspace-window-test"));
    command
        .arg(&path)
        .env("XDG_RUNTIME_DIR", compositor.runtime_dir.path())
        .env("GDK_BACKEND", backend)
        .env("HEARTHSPACE_WINDOW_TEST_MAXIMIZED", "1")
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log);
    if backend == "wayland" {
        command
            .env(
                "WAYLAND_DISPLAY",
                format!("hearthspace-test-{}", std::process::id()),
            )
            .env_remove("DISPLAY");
    } else {
        let deadline = Instant::now() + Duration::from_secs(10);
        let display = loop {
            let status = query(compositor, "status");
            if let Some(display) = status["xwayland_display"].as_str() {
                break display.to_string();
            }
            assert!(Instant::now() < deadline, "XWayland not ready");
            thread::sleep(Duration::from_millis(25));
        };
        command
            .env("DISPLAY", display)
            .env_remove("WAYLAND_DISPLAY");
    }
    // Own the process before any assertion so failed startup checks reap it too.
    let editors = Editors(vec![command.spawn().unwrap()]);
    let is_new = |window: &Value| !existing_ids.contains(&window["id"].as_u64().unwrap());
    let windows = wait_windows(compositor, |windows| {
        windows.iter().any(|window| {
            is_new(window)
                && window["mapped"] == true
                && window["focused"] == true
                && window["maximized"] == true
                && window["focused_view"] == true
                && window["fullscreen"] == false
                && window["x"] == 0
                && window["y"] == 0
                && window["width"] == 1280
                && window["height"] == 800
        })
    });
    let focused = windows.iter().find(|window| is_new(window)).unwrap();
    let id = focused["id"].as_u64().unwrap();
    wait_state(&path, |state| {
        state["active"] == true
            && state["maximized"] == true
            && state["fullscreen"] == false
            && state["width"] == focused["content_width"]
            && state["height"] == focused["content_height"]
    });

    // Only the new fixture should receive the shortcut, including later launches.
    keys(compositor, &[125, 50]); // Super+M explicitly enters bounded canvas mode.
    let windows = wait_windows(compositor, |windows| {
        windows.iter().any(|window| {
            window["id"] == id
                && window["focused"] == true
                && window["maximized"] == true
                && window["focused_view"] == false
                && window["fullscreen"] == false
                && window["x"] == 24
                && window["y"] == 96
                && window["width"] == 1232
                && window["height"] == 608
        })
    });
    let canvas = windows.iter().find(|window| window["id"] == id).unwrap();
    wait_state(&path, |state| {
        state["maximized"] == true
            && state["fullscreen"] == false
            && state["width"] == canvas["content_width"]
            && state["height"] == canvas["content_height"]
    });
    (editors, path)
}

pub(super) fn wait_state(path: &Path, condition: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = std::fs::read(path)
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or(Value::Null);
        if condition(&state) {
            return state;
        }
        assert!(Instant::now() < deadline, "client state timed out: {state}");
        thread::sleep(Duration::from_millis(25));
    }
}
