use super::navigation::{Editors, action, keys, query, wait_windows};
use super::*;
use serde_json::Value;

#[track_caller]
fn wait_state(path: &Path, condition: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .unwrap_or(Value::Null);
        if condition(&state) {
            return state;
        }
        assert!(Instant::now() < deadline, "client state timed out: {state}");
        thread::sleep(Duration::from_millis(25));
    }
}

fn fixture(compositor: &HeadlessCompositor, backend: &str, state: &Path) -> Editors {
    fixture_with_size(compositor, backend, state, None)
}

fn fixture_with_size(
    compositor: &HeadlessCompositor,
    backend: &str,
    state: &Path,
    size: Option<&str>,
) -> Editors {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hearthspace-window-test"));
    command
        .arg(state)
        .env("XDG_RUNTIME_DIR", compositor.runtime_dir.path())
        .env("GDK_BACKEND", backend);
    if let Some(size) = size {
        command.env("HEARTHSPACE_WINDOW_TEST_SIZE", size);
    }
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
    let log =
        std::fs::File::create(compositor.runtime_dir.path().join("window-fixture.log")).unwrap();
    Editors(vec![
        command
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    ])
}

fn assert_green_pixel(compositor: &HeadlessCompositor, x: usize, y: usize) {
    let bytes = compositor.take_screenshot();
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut data = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut data).unwrap();
    let channels = info.color_type.samples();
    let offset = (y * info.width as usize + x) * channels;
    assert_eq!(
        &data[offset..offset + 3],
        &[0, 204, 102],
        "fullscreen rendered content at {x},{y}"
    );
}

fn wait_for_complete_popup(compositor: &HeadlessCompositor) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()));
        let mut reader = decoder.read_info().unwrap();
        let mut data = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut data).unwrap();
        let red = data[..info.buffer_size()]
            .chunks_exact(info.color_type.samples())
            .filter(|pixel| pixel[..3] == [220, 20, 60])
            .count();
        // The 240x160 popup button would lose over half its area at the
        // bottom-right anchor without the protocol's flip/slide constraints.
        if red > 30_000 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "edge popup is clipped or missing: {red} red pixels"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn run_window_states(backend: &str) {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let state_path = compositor.runtime_dir.path().join("window-state.json");
    let _fixture = fixture(&compositor, backend, &state_path);
    let windows = wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["focused"] == true
    });
    let original = windows[0].clone();
    let id = original["id"].as_u64().unwrap();
    wait_state(&state_path, |state| state["active"] == true);

    // A zoomed canvas must not shrink output-pinned windows or their input.
    action(&compositor, "zoom-out");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if query(&compositor, "status")["viewport"]["scale"]
            .as_f64()
            .unwrap()
            < 0.99
        {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(25));
    }
    keys(&compositor, &[67]); // F9: client requests maximize.
    wait_state(&state_path, |state| state["maximized"] == true);
    wait_windows(&compositor, |windows| {
        windows[0]["maximized"] == true
            && windows[0]["x"] == 0
            && windows[0]["y"] == 48
            && windows[0]["width"] == 1280
            && windows[0]["height"] == 752
    });
    assert!(
        compositor
            .send_text_command(&format!("move-window {id} 0 0"))
            .starts_with("err ")
    );

    keys(&compositor, &[87]); // F11: client requests fullscreen, retaining max state.
    wait_state(&state_path, |state| state["fullscreen"] == true);
    wait_windows(&compositor, |windows| {
        windows[0]["fullscreen"] == true
            && windows[0]["x"] == 0
            && windows[0]["y"] == 0
            && windows[0]["width"] == 1280
            && windows[0]["height"] == 800
    });
    assert_green_pixel(&compositor, 1200, 700);
    action(&compositor, "pointer-motion-abs 1100 700");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    keys(&compositor, &[30]);
    wait_state(&state_path, |state| state["text"] == "a");

    keys(&compositor, &[125, 33]); // Super+F uses the compositor's same state action.
    wait_state(&state_path, |state| {
        state["fullscreen"] == false && state["maximized"] == true
    });
    wait_windows(&compositor, |windows| {
        windows[0]["height"] == 752 && windows[0]["y"] == 48
    });
    action(&compositor, format!("restore-window {id}"));
    wait_state(&state_path, |state| {
        state["maximized"] == false && state["fullscreen"] == false
    });
    wait_windows(&compositor, |windows| {
        windows[0]["content_width"] == original["content_width"]
            && windows[0]["content_height"] == original["content_height"]
            && windows[0]["canvas_x"] == original["canvas_x"]
            && windows[0]["canvas_y"] == original["canvas_y"]
    });

    action(&compositor, format!("fullscreen-window {id} true"));
    wait_state(&state_path, |state| state["fullscreen"] == true);
    keys(&compositor, &[64]); // F6: a real transient dialog above fullscreen.
    let windows = wait_windows(&compositor, |windows| {
        windows
            .iter()
            .any(|window| window["title"] == "Window fixture dialog" && window["focused"] == true)
    });
    let dialog = windows
        .iter()
        .find(|window| window["title"] == "Window fixture dialog")
        .unwrap();
    assert!(dialog["x"].as_i64().unwrap() >= 0 && dialog["y"].as_i64().unwrap() >= 0);
    assert!(dialog["x"].as_i64().unwrap() + dialog["width"].as_i64().unwrap() <= 1280);
    assert!(dialog["y"].as_i64().unwrap() + dialog["height"].as_i64().unwrap() <= 800);
    assert!(
        compositor
            .send_text_command(&format!(
                "move-window {} 0 0",
                dialog["id"].as_u64().unwrap()
            ))
            .starts_with("err restore the parent")
    );
    action(
        &compositor,
        format!("close-window {}", dialog["id"].as_u64().unwrap()),
    );
    wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["focused"] == true
    });
    wait_state(&state_path, |state| state["active"] == true);

    if backend == "wayland" {
        keys(&compositor, &[65]); // F7: popup anchored at bottom-right output edge.
        wait_state(&state_path, |state| state["popup"] == true);
        wait_for_complete_popup(&compositor);
        // Grabs retain authority; agent actions return a clear error until dismissal.
        assert!(
            compositor
                .send_text_command(&format!("focus-window {id}"))
                .starts_with("err ")
        );
        keys(&compositor, &[1]);
        wait_state(&state_path, |state| state["popup"] == false);
        // The client hides its widget before the server necessarily sees its
        // popup destroy. Observe released grab authority, without extra input
        // that could conceal a stale pointer grab.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let reply = compositor.send_text_command(&format!("focus-window {id}"));
            if reply == "ok\n" {
                break;
            }
            assert!(
                reply.starts_with("err window action unavailable"),
                "{reply}"
            );
            assert!(
                Instant::now() < deadline,
                "popup grab did not retire: {reply}"
            );
            thread::sleep(Duration::from_millis(25));
        }
    }
    action(&compositor, format!("close-window {id}"));
    wait_windows(&compositor, |windows| windows.is_empty());
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn window_states_wayland_maximize_fullscreen_restore_and_dialogs() {
    run_window_states("wayland");
}

#[test]
fn window_bounds_wayland_oversized_initial_window_and_dialog_fit_output() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let state_path = compositor.runtime_dir.path().join("window-state.json");
    let _fixture = fixture_with_size(&compositor, "wayland", &state_path, Some("1600x1200"));
    let fits = |window: &Value| {
        let x = window["x"].as_i64().unwrap();
        let y = window["y"].as_i64().unwrap();
        let width = window["width"].as_i64().unwrap();
        let height = window["height"].as_i64().unwrap();
        x >= 0 && y >= 48 && width > 0 && height > 0 && x + width <= 1280 && y + height <= 800
    };
    let windows = wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["focused"] == true && fits(&windows[0])
    });
    let id = windows[0]["id"].as_u64().unwrap();
    wait_state(&state_path, |state| state["active"] == true);
    keys(&compositor, &[66]); // F8 requests a 1600x1200 transient window.
    let windows = wait_windows(&compositor, |windows| {
        windows.len() == 2
            && windows.iter().all(fits)
            && windows.iter().any(|window| {
                window["title"] == "Window fixture dialog" && window["focused"] == true
            })
    });
    let dialog = windows.iter().find(|window| window["id"] != id).unwrap();
    action(&compositor, format!("close-window {}", dialog["id"]));
    wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["focused"] == true
    });
    // Suggested initial bounds must not become a permanent canvas size limit.
    action(&compositor, format!("resize-window {id} 1500 900"));
    wait_windows(&compositor, |windows| {
        windows[0]["content_width"] == 1500 && windows[0]["content_height"] == 900
    });
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[cfg(feature = "xwayland")]
#[test]
fn window_states_x11_maximize_fullscreen_restore_and_dialogs() {
    run_window_states("x11");
}
