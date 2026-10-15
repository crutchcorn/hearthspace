//! Canvas zoom must remain visible when an ordinary app starts maximized.
use super::navigation::{Editors, action, keys, query, wait_windows};
use super::*;
use serde_json::Value;

fn maximized_fixture(
    compositor: &HeadlessCompositor,
    name: &str,
    backend: &str,
) -> (Editors, PathBuf) {
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
    let child = command.spawn().unwrap();
    (Editors(vec![child]), path)
}

fn wait_state(path: &Path, condition: impl Fn(&Value) -> bool) -> Value {
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

fn wait_scale(compositor: &HeadlessCompositor, target: f64) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let scale = query(compositor, "status")["viewport"]["scale"]
            .as_f64()
            .unwrap();
        if (scale - target).abs() < 0.0001 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "expected scale {target}, got {scale}"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn green_pixels(screenshot: &[u8]) -> usize {
    let decoder = png::Decoder::new(std::io::Cursor::new(screenshot));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    bytes[..info.buffer_size()]
        .chunks_exact(info.color_type.samples())
        .filter(|pixel| pixel[..3] == [0, 204, 102])
        .count()
}

fn wait_green_pixels(
    compositor: &HeadlessCompositor,
    phase: &str,
    condition: impl Fn(usize) -> bool,
) -> usize {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let screenshot = compositor.take_screenshot();
        let pixels = green_pixels(&screenshot);
        if condition(pixels) {
            return pixels;
        }
        if Instant::now() >= deadline {
            let path = compositor.runtime_dir.path().join(format!("{phase}.png"));
            std::fs::write(&path, screenshot).unwrap();
            panic!(
                "{phase}: rendered fixture did not reach its expected size; \
                 {pixels} green pixels, screenshot: {}",
                path.display()
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn run_zoom_restores_startup_maximized_apps(backend: &str) {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let (_first, state) = maximized_fixture(&compositor, "first", backend);
    let first = wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["maximized"] == true && windows[0]["width"] == 1280
    })[0]
        .clone();
    wait_state(&state, |state| state["maximized"] == true);
    let id = first["id"].as_u64().unwrap();
    // GTK reports its maximized state before the corresponding buffer arrives,
    // especially through XWayland/software rendering. Require the green editor
    // to visibly fill most of the output before using it as our baseline.
    let before_pixels =
        wait_green_pixels(&compositor, "maximized", |pixels| pixels > 1280 * 800 / 2);

    action(&compositor, "zoom-out");
    wait_scale(&compositor, 0.8);
    wait_state(&state, |state| {
        state["maximized"] == false && state["active"] == true
    });
    let floating = wait_windows(&compositor, |windows| {
        windows[0]["maximized"] == false
            && windows[0]["focused"] == true
            && windows[0]["visible"] == true
            && windows[0]["content_width"] == 560
            && windows[0]["content_height"] == 360
    })[0]
        .clone();
    assert_eq!(floating["canvas_x"], first["canvas_x"]);
    assert_eq!(floating["canvas_y"], first["canvas_y"]);
    assert!(floating["width"].as_i64().unwrap() < 1280);
    // State/configure acknowledgement likewise precedes the restored buffer.
    // A missing app must not satisfy the shrink check merely by drawing nothing.
    wait_green_pixels(&compositor, "restored", |pixels| {
        pixels > 0 && pixels < before_pixels / 2
    });
    let center_x = floating["x"].as_f64().unwrap() + floating["width"].as_f64().unwrap() / 2.0;
    let center_y = floating["y"].as_f64().unwrap() + floating["height"].as_f64().unwrap() / 2.0;
    action(
        &compositor,
        format!("pointer-motion-abs {center_x} {center_y}"),
    );
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    keys(&compositor, &[30]);
    wait_state(&state, |state| state["text"] == "a");

    // A distant saved position is retained, while zoom reveals it before
    // restoring; the configure response must not later undo the zoom.
    action(&compositor, format!("move-window {id} 7000 -3000"));
    action(&compositor, format!("maximize-window {id} true"));
    wait_state(&state, |state| state["maximized"] == true);
    let (_second, second_state) = maximized_fixture(&compositor, "second", backend);
    let windows = wait_windows(&compositor, |windows| {
        windows.len() == 2 && windows.iter().all(|window| window["maximized"] == true)
    });
    let second_id = windows.iter().find(|window| window["id"] != id).unwrap()["id"]
        .as_u64()
        .unwrap();
    action(&compositor, format!("focus-window {id}"));
    action(&compositor, "key-down 125");
    action(&compositor, "axis 0 -15");
    action(&compositor, "key-up 125");
    let windows = wait_windows(&compositor, |windows| {
        windows.iter().all(|window| window["maximized"] == false)
            && windows.iter().any(|window| {
                window["id"] == id && window["focused"] == true && window["visible"] == true
            })
    });
    let restored = windows.iter().find(|window| window["id"] == id).unwrap();
    assert_eq!(restored["canvas_x"], 7000);
    assert_eq!(restored["canvas_y"], -3000);
    wait_state(&state, |state| {
        state["maximized"] == false && state["width"] == 560 && state["height"] == 360
    });
    wait_state(&second_state, |state| state["maximized"] == false);
    assert!(
        query(&compositor, "status")["viewport"]["scale"]
            .as_f64()
            .unwrap()
            > 0.8
    );
    action(&compositor, format!("close-window {second_id}"));
    action(&compositor, format!("close-window {id}"));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn zoom_restores_startup_maximized_wayland_apps_and_preserves_floating_input() {
    run_zoom_restores_startup_maximized_apps("wayland");
}

#[test]
fn zoom_restores_startup_maximized_x11_apps_and_preserves_floating_input() {
    run_zoom_restores_startup_maximized_apps("x11");
}

#[test]
fn zoom_preserves_fullscreen_and_maximization_at_scale_limit() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let (_fixture, state) = maximized_fixture(&compositor, "fullscreen", "wayland");
    let first = wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["maximized"] == true
    })[0]
        .clone();
    let id = first["id"].as_u64().unwrap();
    action(&compositor, format!("fullscreen-window {id} true"));
    wait_state(&state, |state| state["fullscreen"] == true);
    action(&compositor, "zoom-out");
    wait_scale(&compositor, 0.8);
    wait_windows(&compositor, |windows| {
        windows[0]["fullscreen"] == true
            && windows[0]["maximized"] == true
            && windows[0]["x"] == 0
            && windows[0]["y"] == 0
            && windows[0]["width"] == 1280
            && windows[0]["height"] == 800
    });
    for target in [0.64, 0.512, 0.5] {
        action(&compositor, "zoom-out");
        wait_scale(&compositor, target);
    }
    action(&compositor, format!("fullscreen-window {id} false"));
    wait_state(&state, |state| {
        state["fullscreen"] == false && state["maximized"] == true
    });
    action(&compositor, "zoom-out");
    wait_scale(&compositor, 0.5);
    let windows = query(&compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(windows[0]["maximized"], true);
    assert_eq!(windows[0]["width"], 1280);
    action(&compositor, format!("close-window {id}"));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn zoom_reveals_a_focused_dialog_above_a_distant_maximized_parent() {
    let _guard = HEADLESS_TEST_LOCK.lock().unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let (_fixture, state) = maximized_fixture(&compositor, "dialog-parent", "wayland");
    let first = wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["maximized"] == true
    })[0]
        .clone();
    let id = first["id"].as_u64().unwrap();
    action(&compositor, format!("restore-window {id}"));
    wait_state(&state, |state| {
        state["maximized"] == false && state["width"] == 560
    });
    action(&compositor, format!("move-window {id} 7000 -3000"));
    action(&compositor, format!("maximize-window {id} true"));
    wait_state(&state, |state| state["maximized"] == true);
    keys(&compositor, &[64]); // F6 opens a real modal transient.
    let windows = wait_windows(&compositor, |windows| {
        windows.len() == 2
            && windows.iter().any(|window| {
                window["title"] == "Window fixture dialog" && window["focused"] == true
            })
    });
    let dialog = windows
        .iter()
        .find(|window| window["title"] == "Window fixture dialog")
        .unwrap();
    let dialog_id = dialog["id"].as_u64().unwrap();
    action(&compositor, "zoom-out");
    wait_scale(&compositor, 0.8);
    wait_state(&state, |state| state["maximized"] == false);
    let windows = wait_windows(&compositor, |windows| {
        windows.iter().all(|window| window["maximized"] == false)
            && windows.iter().any(|window| {
                window["id"] == dialog_id && window["focused"] == true && window["visible"] == true
            })
    });
    let restored_parent = windows.iter().find(|window| window["id"] == id).unwrap();
    assert_eq!(restored_parent["canvas_x"], 7000);
    assert_eq!(restored_parent["canvas_y"], -3000);
    action(&compositor, format!("close-window {dialog_id}"));
    action(&compositor, format!("close-window {id}"));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[path = "canvas_zoom/shell_bar.rs"]
mod shell_bar;
