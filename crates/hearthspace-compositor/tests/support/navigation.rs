//! Real-client coverage for window actions and the shared physical/agent key policy.
use super::*;
use serde_json::Value;

#[path = "navigation_close.rs"]
mod close;

pub(super) struct Editors(pub(super) Vec<Child>);

impl Drop for Editors {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(super) fn query(compositor: &HeadlessCompositor, command: &str) -> Value {
    let mut stream = UnixStream::connect(&compositor.command_socket_path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(format!("{command}\n").as_bytes()).unwrap();
    let header = read_line(&mut stream);
    let size: usize = header
        .strip_prefix("ok ")
        .expect("JSON response")
        .trim()
        .parse()
        .unwrap();
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

pub(super) fn wait_windows(
    compositor: &HeadlessCompositor,
    condition: impl Fn(&[Value]) -> bool,
) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let windows = query(compositor, "windows")["windows"]
            .as_array()
            .unwrap()
            .clone();
        if condition(&windows) {
            return windows;
        }
        assert!(
            Instant::now() < deadline,
            "window condition timed out: {windows:?}"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn action(compositor: &HeadlessCompositor, command: impl AsRef<str>) {
    assert_eq!(
        compositor.send_text_command(command.as_ref()),
        "ok\n",
        "{}",
        command.as_ref()
    );
}

pub(super) fn keys(compositor: &HeadlessCompositor, codes: &[u32]) {
    for code in codes {
        action(compositor, format!("key-down {code}"));
    }
    for code in codes.iter().rev() {
        action(compositor, format!("key-up {code}"));
    }
}

fn wait_activation(compositor: &HeadlessCompositor, active: usize, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let states: Vec<Value> = (0..count)
            .map(|number| {
                serde_json::from_slice(
                    &std::fs::read(
                        compositor
                            .runtime_dir
                            .path()
                            .join(format!("editor-{number}.json")),
                    )
                    .unwrap(),
                )
                .unwrap()
            })
            .collect();
        if states
            .iter()
            .enumerate()
            .all(|(number, state)| state["active"] == (number == active))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "client activation disagrees with keyboard focus: {states:?}"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn navigation_maps_focuses_cycles_reveals_resizes_and_closes_real_clients() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let mut ids = Vec::new();
    for number in 0..3 {
        let title = format!("Navigation editor {number}");
        let input = compositor
            .runtime_dir
            .path()
            .join(format!("input-{number}.txt"));
        let output = compositor
            .runtime_dir
            .path()
            .join(format!("editor-{number}.json"));
        std::fs::write(&input, "").unwrap();
        let log = std::fs::File::create(
            compositor
                .runtime_dir
                .path()
                .join(format!("editor-{number}.log")),
        )
        .unwrap();
        editors.0.push(
            Command::new(env!("CARGO_BIN_EXE_hearthspace-clipboard-test"))
                .args([
                    "--title",
                    &title,
                    "--input-file",
                    input.to_str().unwrap(),
                    "--output-file",
                    output.to_str().unwrap(),
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
                .unwrap(),
        );
        let windows = wait_windows(&compositor, |windows| {
            windows.len() == number + 1
                && windows
                    .iter()
                    .any(|window| window["title"] == title && window["focused"] == true)
        });
        let id = windows
            .iter()
            .find(|window| window["title"] == title)
            .unwrap()["id"]
            .as_u64()
            .unwrap();
        ids.push(id);
        assert_eq!(
            windows
                .iter()
                .filter(|window| window["focused"] == true)
                .count(),
            1
        );
        wait_activation(&compositor, number, number + 1);
    }

    // Each completed chord raises its choice, so quick Alt+Tab toggles the
    // two most recent windows instead of continuing a previous session.
    for id in [ids[1], ids[2]] {
        keys(&compositor, &[56, 15]); // Alt+Tab uses the same handler as real keys.
        wait_windows(&compositor, |windows| {
            windows
                .iter()
                .any(|window| window["id"] == id && window["focused"] == true)
        });
    }
    // Reverse starts at the oldest window in each fresh front-to-back list.
    for id in [ids[0], ids[1]] {
        keys(&compositor, &[56, 42, 15]);
        wait_windows(&compositor, |windows| {
            windows
                .iter()
                .any(|window| window["id"] == id && window["focused"] == true)
        });
    }
    wait_activation(&compositor, 1, 3);

    action(&compositor, format!("move-window {} 9000 -7000", ids[0]));
    wait_windows(&compositor, |windows| {
        windows
            .iter()
            .any(|window| window["id"] == ids[0] && window["visible"] == false)
    });
    action(&compositor, format!("focus-window {}", ids[0]));
    let windows = wait_windows(&compositor, |windows| {
        windows.iter().any(|window| {
            window["id"] == ids[0] && window["visible"] == true && window["focused"] == true
        })
    });
    let focused = windows
        .iter()
        .find(|window| window["id"] == ids[0])
        .unwrap();
    assert_eq!(focused["canvas_x"], 9000);
    assert_eq!(focused["canvas_y"], -7000);
    assert_eq!(focused["client_type"], "wayland");
    assert_eq!(focused["mapped"], true);
    assert!(!focused["outputs"].as_array().unwrap().is_empty());
    action(&compositor, format!("resize-window {} 700 480", ids[0]));
    wait_windows(&compositor, |windows| {
        windows.iter().any(|window| {
            window["id"] == ids[0]
                && window["content_width"] == 700
                && window["content_height"] == 480
        })
    });
    keys(&compositor, &[30]); // Typing reaches the newly focused editor.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state: Value = serde_json::from_slice(
            &std::fs::read(compositor.runtime_dir.path().join("editor-0.json")).unwrap(),
        )
        .unwrap();
        if state["text"] == "a" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "typing never reached focused editor: {state}"
        );
        thread::sleep(Duration::from_millis(25));
    }

    for command in [
        "focus-window 999999",
        "resize-window 1 0 100",
        "move-window 1 1000001 0",
    ] {
        assert!(
            compositor.send_text_command(command).starts_with("err "),
            "{command}"
        );
    }
    // Focus history is encoded in stacking order: closing editor zero returns
    // to editor one, the previously focused window, even after moving off-canvas.
    keys(&compositor, &[56, 62]);
    wait_windows(&compositor, |windows| {
        windows.len() == 2
            && windows
                .iter()
                .any(|window| window["id"] == ids[1] && window["focused"] == true)
    });
    action(&compositor, format!("close-window {}", ids[1]));
    wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["id"] == ids[2] && windows[0]["focused"] == true
    });
    action(&compositor, format!("close-window {}", ids[2]));
    wait_windows(&compositor, |windows| windows.is_empty());
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[cfg(feature = "xwayland")]
#[test]
fn navigation_moves_x11_without_doubling_its_global_position() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let deadline = Instant::now() + Duration::from_secs(10);
    let display = loop {
        let status = query(&compositor, "status");
        if let Some(display) = status["xwayland_display"].as_str() {
            break display.to_string();
        }
        assert!(Instant::now() < deadline, "XWayland did not start");
        thread::sleep(Duration::from_millis(25));
    };
    let input = compositor.runtime_dir.path().join("x11-input.txt");
    let output = compositor.runtime_dir.path().join("x11-editor.json");
    std::fs::write(&input, "").unwrap();
    let log = std::fs::File::create(compositor.runtime_dir.path().join("x11-editor.log")).unwrap();
    let _editor = Editors(vec![
        Command::new(env!("CARGO_BIN_EXE_hearthspace-clipboard-test"))
            .args([
                "--title",
                "X11 navigation",
                "--input-file",
                input.to_str().unwrap(),
                "--output-file",
                output.to_str().unwrap(),
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
    ]);
    let windows = wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["focused"] == true
    });
    let id = windows[0]["id"].as_u64().unwrap();
    assert_eq!(windows[0]["client_type"], "x11");
    action(&compositor, format!("move-window {id} 400 200"));
    action(&compositor, "pan-to 200 100");
    let windows = wait_windows(&compositor, |windows| {
        windows[0]["canvas_x"] == 400 && windows[0]["canvas_y"] == 200
    });
    let viewport = &query(&compositor, "status")["viewport"];
    assert_eq!(
        windows[0]["x"].as_i64().unwrap(),
        400 - viewport["x"].as_i64().unwrap()
    );
    assert_eq!(
        windows[0]["y"].as_i64().unwrap(),
        200 - viewport["y"].as_i64().unwrap()
    );
    // Click coordinates are checked independently of the compositor's metadata:
    // translate the known canvas origin by the reported viewport, then type.
    let x = 450 - viewport["x"].as_i64().unwrap();
    let y = 300 - viewport["y"].as_i64().unwrap();
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    keys(&compositor, &[48]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        if state["text"] == "b" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "X11 input missed moved client: {state}"
        );
        thread::sleep(Duration::from_millis(25));
    }
    action(&compositor, format!("close-window {id}"));
    wait_windows(&compositor, |windows| windows.is_empty());
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
