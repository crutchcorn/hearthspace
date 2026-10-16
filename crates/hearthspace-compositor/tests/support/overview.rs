//! Exercise the overview against real clients, including modal input isolation.
use super::navigation::{Editors, action, keys, query, wait_windows};
use super::session_lock::lock_client;
use super::*;
use serde_json::{Value, json};

pub(super) fn editor(
    compositor: &HeadlessCompositor,
    editors: &mut Editors,
    number: usize,
) -> (u64, PathBuf) {
    let title = format!("Overview editor {number}");
    let input = compositor
        .runtime_dir
        .path()
        .join(format!("overview-{number}.txt"));
    let state = input.with_extension("json");
    std::fs::write(&input, "").unwrap();
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
    let windows = wait_windows(compositor, |windows| {
        windows.iter().any(|window| {
            window["title"] == title && window["mapped"] == true && window["focused"] == true
        })
    });
    let id = windows
        .iter()
        .find(|window| window["title"] == title)
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    wait_state(&state, |value| {
        value["ready"] == true && value["active"] == true
    });
    (id, state)
}

pub(super) fn wait_state(path: &Path, condition: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or(Value::Null);
        if condition(&state) {
            return state;
        }
        assert!(Instant::now() < deadline, "editor state timed out: {state}");
        thread::sleep(Duration::from_millis(20));
    }
}

fn overview(compositor: &HeadlessCompositor, active: bool) -> Value {
    let status = query(compositor, "status");
    assert_eq!(status["overview"]["active"], active, "{status}");
    status
}

fn layout(status: &Value) -> Value {
    let mut windows: Vec<_> = status["windows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|window| {
            json!({"id":window["id"], "canvas_x":window["canvas_x"], "canvas_y":window["canvas_y"],
            "content_width":window["content_width"], "content_height":window["content_height"],
            "maximized":window["maximized"], "fullscreen":window["fullscreen"],
            "presentation_output":window["presentation_output"]})
        })
        .collect();
    windows.sort_by_key(|window| window["id"].as_u64().unwrap());
    json!({"viewport":status["viewport"],"windows":windows})
}

#[test]
fn overview_keys_are_modal_and_dismissal_restores_real_client_input() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let (first, first_state) = editor(&compositor, &mut editors, 0);
    let (second, second_state) = editor(&compositor, &mut editors, 1);
    let before = layout(&query(&compositor, "status"));

    keys(&compositor, &[125, 24]); // Super+O opens through the normal key policy.
    let opened = overview(&compositor, true);
    assert_eq!(opened["overview"]["selected_window_id"], second);
    assert!(
        opened["windows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|window| window["focused"] == false)
    );
    wait_state(&second_state, |value| value["active"] == false);
    keys(&compositor, &[30]); // A is consumed rather than typed into the old client.
    keys(&compositor, &[56, 15]); // Alt+Tab must not escape the modal view.
    keys(&compositor, &[125, 28]); // Nor should Super+Enter launch a terminal.
    action(&compositor, "axis 0 -120");
    assert_eq!(layout(&overview(&compositor, true)), before);
    keys(&compositor, &[1]); // Escape dismisses, retaining the original focus.
    overview(&compositor, false);
    wait_windows(&compositor, |windows| {
        windows
            .iter()
            .any(|window| window["id"] == second && window["focused"] == true)
    });
    keys(&compositor, &[48]);
    // This delivered B is an ordering witness: an earlier leaked A would leave AB.
    wait_state(&second_state, |value| value["text"] == "b");
    assert_eq!(
        wait_state(&first_state, |value| value["ready"] == true)["text"],
        ""
    );

    action(&compositor, "overview-on");
    action(&compositor, "key-down 30");
    keys(&compositor, &[125, 24]); // The toggle also closes an existing overview.
    action(&compositor, "key-up 30"); // Release after dismissal stays consumed.
    overview(&compositor, false);
    keys(&compositor, &[46]);
    wait_state(&second_state, |value| value["text"] == "bc");

    action(&compositor, "overview-on");
    action(&compositor, "key-down 42"); // Shift is still held when focus returns.
    keys(&compositor, &[1]);
    action(&compositor, "key-up 42");
    keys(&compositor, &[18]);
    // A consumed release must still advertise the cleared modifier state.
    wait_state(&second_state, |value| value["text"] == "bce");

    action(&compositor, "overview-toggle");
    assert!(
        compositor
            .send_text_command("overview-select 999999")
            .starts_with("err ")
    );
    keys(&compositor, &[15]);
    assert_eq!(
        overview(&compositor, true)["overview"]["selected_window_id"],
        first
    );
    keys(&compositor, &[28]);
    overview(&compositor, false);
    wait_windows(&compositor, |windows| {
        windows
            .iter()
            .any(|window| window["id"] == first && window["focused"] == true)
    });
    keys(&compositor, &[32]);
    wait_state(&first_state, |value| value["text"] == "d");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn overview_preserves_presentation_tracks_closed_clients_and_selects_off_canvas() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let (first, first_state) = editor(&compositor, &mut editors, 0);
    let (second, _) = editor(&compositor, &mut editors, 1);
    action(&compositor, format!("move-window {first} 9000 -7000"));
    action(&compositor, format!("fullscreen-window {second} true"));
    wait_windows(&compositor, |windows| {
        windows.iter().any(|window| {
            window["id"] == second
                && window["fullscreen"] == true
                && window["content_width"] == 1280
        })
    });
    let before = query(&compositor, "status");
    let image = compositor.take_screenshot();
    action(&compositor, "overview-on");
    action(&compositor, "overview-on"); // Repeated open is idempotent.
    let opened = overview(&compositor, true);
    assert_eq!(layout(&opened), layout(&before));
    let items = opened["overview"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    for item in items {
        assert!(item["preview_width"].as_i64().unwrap() > 0);
        assert!(item["preview_height"].as_i64().unwrap() > 0);
        assert!(item["x"].as_i64().unwrap() >= 0 && item["y"].as_i64().unwrap() >= 0);
    }
    assert_ne!(
        compositor.take_screenshot(),
        image,
        "overview never appeared in rendered output"
    );
    for command in [
        format!("focus-window {first}"),
        format!("resize-window {first} 300 200"),
    ] {
        assert!(
            compositor.send_text_command(&command).starts_with("err "),
            "{command}"
        );
    }
    action(&compositor, "overview-off");
    action(&compositor, "overview-off");
    assert_eq!(layout(&overview(&compositor, false)), layout(&before));

    action(&compositor, "overview-on");
    editors.0[1].kill().unwrap();
    editors.0[1].wait().unwrap();
    wait_windows(&compositor, |windows| windows.len() == 1);
    let deadline = Instant::now() + Duration::from_secs(5);
    let item = loop {
        let state = overview(&compositor, true);
        let items = state["overview"]["items"].as_array().unwrap();
        if items.len() == 1 && state["overview"]["selected_window_id"] == first {
            break items[0].clone();
        }
        assert!(
            Instant::now() < deadline,
            "overview retained a closed client: {state}"
        );
        thread::sleep(Duration::from_millis(20));
    };
    assert!(
        compositor
            .send_text_command(&format!("overview-select {second}"))
            .starts_with("err ")
    );
    let x = item["x"].as_f64().unwrap() + item["width"].as_f64().unwrap() / 2.0;
    let y = item["y"].as_f64().unwrap() + item["height"].as_f64().unwrap() / 2.0;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 272");
    overview(&compositor, true);
    action(&compositor, "pointer-button-up 272");
    overview(&compositor, false);
    let windows = wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["visible"] == true && windows[0]["focused"] == true
    });
    assert_eq!(windows[0]["canvas_x"], 9000);
    assert_eq!(windows[0]["canvas_y"], -7000);
    keys(&compositor, &[30]);
    wait_state(&first_state, |value| value["text"] == "a");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn session_lock_dismisses_overview_and_redacts_titles_and_selection() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("800x600");
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let (id, state) = editor(&compositor, &mut editors, 0);
    action(&compositor, "overview-on");
    assert_eq!(
        overview(&compositor, true)["overview"]["items"][0]["title"],
        "Overview editor 0"
    );
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    let locked = query(&compositor, "status");
    assert_eq!(locked["locked"], true);
    assert!(locked["windows"].as_array().unwrap().is_empty());
    assert!(locked.get("overview").is_none());
    assert!(!locked.to_string().contains("Overview editor 0"));
    for command in [
        "overview-on".into(),
        "overview-off".into(),
        "overview-toggle".into(),
        format!("overview-select {id}"),
    ] {
        assert!(
            compositor
                .send_text_command(&command)
                .starts_with("err session is locked"),
            "{command}"
        );
    }
    locker.unlock();
    overview(&compositor, false);
    wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["id"] == id && windows[0]["focused"] == true
    });
    keys(&compositor, &[48]);
    wait_state(&state, |value| value["text"] == "b");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
