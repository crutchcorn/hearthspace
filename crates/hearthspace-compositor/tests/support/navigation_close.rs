//! Losing a focused client restores keyboard focus without navigating the canvas.
use super::super::{interactive::client::InteractiveClient, navigation_client::wait_state};
use super::*;
use serde_json::json;

fn editor(
    compositor: &HeadlessCompositor,
    editors: &mut Editors,
    number: usize,
    backend: &str,
) -> (u64, PathBuf) {
    if backend == "wayland" {
        return super::super::navigation_client::editor(compositor, editors, number);
    }
    let title = format!("Close navigation editor {number}");
    let input = compositor
        .runtime_dir
        .path()
        .join(format!("close-{number}.txt"));
    let state = input.with_extension("json");
    std::fs::write(&input, "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let display = loop {
        let status = query(compositor, "status");
        if let Some(display) = status["xwayland_display"].as_str() {
            break display.to_owned();
        }
        assert!(Instant::now() < deadline, "XWayland did not start");
        thread::sleep(Duration::from_millis(25));
    };
    let log = std::fs::File::create(input.with_extension("log")).unwrap();
    editors.0.push(
        Command::new(env!("CARGO_BIN_EXE_hearthspace-clipboard-test"))
            .args([
                "--title",
                &title,
                "--input-file",
                input.to_str().unwrap(),
                "--output-file",
                state.to_str().unwrap(),
            ])
            .env("XDG_RUNTIME_DIR", compositor.runtime_dir.path())
            .env("DISPLAY", display)
            .env("GDK_BACKEND", "x11")
            .env_remove("WAYLAND_DISPLAY")
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let windows = wait_windows(compositor, |windows| {
        windows.iter().any(|window| {
            window["title"] == title && window["mapped"] == true && window["focused"] == true
        })
    });
    let window = windows
        .iter()
        .find(|window| window["title"] == title)
        .unwrap();
    assert_eq!(window["client_type"], backend);
    wait_state(&state, |value| {
        value["ready"] == true && value["active"] == true
    });
    (window["id"].as_u64().unwrap(), state)
}

fn settled_viewport(compositor: &HeadlessCompositor) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let viewport = query(compositor, "status")["viewport"].clone();
        if viewport["animating"] == false {
            return viewport;
        }
        assert!(
            Instant::now() < deadline,
            "viewport did not settle: {viewport}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn parked_viewport(compositor: &HeadlessCompositor) -> Value {
    action(compositor, "zoom-to 0.8");
    settled_viewport(compositor);
    action(compositor, "pan-to 120 80");
    let viewport = settled_viewport(compositor);
    assert_eq!(viewport["x"], 120);
    assert_eq!(viewport["y"], 80);
    assert_eq!(viewport["scale"], 0.8);
    viewport
}

#[track_caller]
fn assert_stationary(compositor: &HeadlessCompositor, expected: &Value, reason: &str) {
    // Observe across frames as well as immediately, so a queued reveal animation
    // cannot pass by returning the old coordinates on its first frame.
    let deadline = Instant::now() + Duration::from_millis(350);
    loop {
        let viewport = query(compositor, "status")["viewport"].clone();
        for field in ["x", "y", "scale", "animating"] {
            assert_eq!(viewport[field], expected[field], "{reason}: {viewport}");
        }
        if Instant::now() >= deadline {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_focus(compositor: &HeadlessCompositor, id: u64, count: usize) -> Vec<Value> {
    wait_windows(compositor, |windows| {
        windows.len() == count
            && windows
                .iter()
                .filter(|window| window["focused"] == true)
                .count()
                == 1
            && windows
                .iter()
                .any(|window| window["id"] == id && window["focused"] == true)
    })
}

fn closing_preserves_viewport(backend: &str) {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let (successor, successor_state) = editor(&compositor, &mut editors, 0, backend);
    let (focused, focused_state) = editor(&compositor, &mut editors, 1, backend);
    let (background, _) = editor(&compositor, &mut editors, 2, backend);
    action(&compositor, format!("move-window {successor} 9000 -7000"));
    action(&compositor, format!("move-window {focused} 220 180"));
    action(&compositor, format!("focus-window {focused}"));
    wait_focus(&compositor, focused, 3);
    let before = parked_viewport(&compositor);

    action(&compositor, format!("close-window {background}"));
    wait_focus(&compositor, focused, 2);
    wait_state(&focused_state, |state| state["active"] == true);
    assert_stationary(
        &compositor,
        &before,
        "closing a nonfocused client moved the canvas",
    );

    // Closing the focused app must hand off activation and input even though
    // its only successor is far beyond the current viewport in both axes.
    action(&compositor, format!("close-window {focused}"));
    wait_focus(&compositor, successor, 1);
    wait_state(&successor_state, |state| state["active"] == true);
    keys(&compositor, &[30]);
    wait_state(&successor_state, |state| state["text"] == "a");
    assert_stationary(
        &compositor,
        &before,
        "closing the focused client revealed its successor",
    );
    let windows = query(&compositor, "windows");
    assert_eq!(windows["windows"][0]["visible"], false);

    // Explicitly selecting that already-focused app is still navigation.
    action(&compositor, format!("focus-window {successor}"));
    wait_windows(&compositor, |windows| {
        windows[0]["focused"] == true && windows[0]["visible"] == true
    });
    let revealed = settled_viewport(&compositor);
    assert_ne!(revealed["x"], before["x"]);
    assert_ne!(revealed["y"], before["y"]);
    assert_eq!(revealed["scale"], before["scale"]);
    keys(&compositor, &[48]);
    wait_state(&successor_state, |state| state["text"] == "ab");

    action(&compositor, format!("close-window {successor}"));
    wait_windows(&compositor, |windows| windows.is_empty());
    assert_stationary(
        &compositor,
        &revealed,
        "closing the last client moved the canvas",
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn wayland_close_restores_keyboard_focus_without_revealing_offscreen_successor() {
    closing_preserves_viewport("wayland");
}

#[cfg(feature = "xwayland")]
#[test]
fn x11_close_restores_keyboard_focus_without_revealing_offscreen_successor() {
    closing_preserves_viewport("x11");
}

#[test]
fn wayland_unmap_and_disconnect_restore_focus_without_moving_viewport() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let (successor, state) = editor(&compositor, &mut editors, 0, "wayland");
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut client = InteractiveClient::connect(&socket);
    let windows = wait_windows(&compositor, |windows| windows.len() == 3);
    let ids: Vec<_> = (0..2)
        .map(|number| {
            windows
                .iter()
                .find(|window| window["title"] == format!("Interactive fixture {number}"))
                .unwrap()["id"]
                .as_u64()
                .unwrap()
        })
        .collect();
    action(&compositor, format!("move-window {successor} 9000 -7000"));
    for id in &ids {
        action(&compositor, format!("move-window {id} 220 180"));
    }
    // Place the editor immediately below the client about to unmap.
    action(&compositor, format!("focus-window {successor}"));
    action(&compositor, format!("focus-window {}", ids[1]));
    let before = parked_viewport(&compositor);
    client.unmap(1);
    let windows = wait_focus(&compositor, successor, 2);
    // The automation window list contains only mapped, navigable windows.
    assert!(windows.iter().all(|window| window["id"] != ids[1]));
    wait_state(&state, |state| state["active"] == true);
    keys(&compositor, &[30]);
    wait_state(&state, |state| state["text"] == "a");
    assert_stationary(
        &compositor,
        &before,
        "null-buffer unmap revealed the successor",
    );

    action(&compositor, format!("focus-window {}", ids[0]));
    let before = parked_viewport(&compositor);
    // Disconnect without answering a close request: both toplevels disappear.
    drop(client);
    let remaining = wait_focus(&compositor, successor, 1);
    assert_eq!(remaining[0]["id"], json!(successor));
    wait_state(&state, |state| state["active"] == true);
    keys(&compositor, &[48]);
    wait_state(&state, |state| state["text"] == "ab");
    assert_stationary(
        &compositor,
        &before,
        "client disconnect revealed the successor",
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
