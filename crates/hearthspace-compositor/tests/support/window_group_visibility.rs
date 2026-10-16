//! Hidden fullscreen chrome must not remain an invisible input target.
use super::super::{interactive::client::InteractiveClient, navigation::wait_windows};
use super::*;

#[test]
fn fullscreen_group_visibility_matches_pointer_and_keyboard_input() {
    let _guard = headless_test_guard();
    let mut compositor =
        HeadlessCompositor::spawn_with_command("1280x800", None, None, 1, false, |command| {
            command.env("HEARTHSPACE_OLLAMA_URL", "http://127.0.0.1:1");
        });
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let (seed, seed_state) = editor(&compositor, &mut editors, 0);
    let (full, full_state) = editor(&compositor, &mut editors, 1);
    select_by_keyboard(&compositor, seed);
    let button = group(&compositor)["gather_button"].clone();
    action(&compositor, format!("fullscreen-window {full} true"));
    wait_windows(&compositor, |windows| {
        windows
            .iter()
            .any(|window| window["id"] == full && window["content_height"] == 800)
    });
    // Hidden group chrome must not remain an invisible input target.
    click(
        &compositor,
        button["x"].as_f64().unwrap() + 20.0,
        button["y"].as_f64().unwrap() + 20.0,
    );
    for code in [30, 57, 34, 1] {
        keys(&compositor, &[code]);
    } // A, Space, G, Escape.
    wait_state(&full_state, |state| state["text"] == "a g");
    let hidden = group(&compositor);
    assert_eq!(hidden["selected"], json!([]));
    assert_eq!(hidden["selecting"], false);
    assert_eq!(hidden["gathering"], false);
    assert_eq!(hidden["error"], Value::Null);

    // Raising a floating app exposes the canvas controls even with a fullscreen
    // window behind it. Selection shortcuts become modal again immediately.
    select_by_keyboard(&compositor, seed);
    action(&compositor, "group-selection-toggle");
    keys(&compositor, &[30]);
    keys(&compositor, &[28]);
    keys(&compositor, &[48]);
    wait_state(&seed_state, |state| state["text"] == "b");
    assert_eq!(group(&compositor)["selecting"], false);
    assert_eq!(group(&compositor)["selected"], json!([seed]));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn fullscreen_during_owned_group_drag_keeps_release_paired() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut client = InteractiveClient::connect(&socket);
    let windows = wait_windows(&compositor, |windows| windows.len() == 2);
    let id = |number| {
        windows
            .iter()
            .find(|window| window["title"] == format!("Interactive fixture {number}"))
            .unwrap()["id"]
            .as_u64()
            .unwrap()
    };
    let (seed, full) = (id(0), id(1));
    select_by_keyboard(&compositor, seed);
    action(&compositor, "group-selection-toggle");
    for code in [15, 57, 28] {
        keys(&compositor, &[code]);
    }
    assert_eq!(group(&compositor)["selected"], json!([seed, full]));
    action(&compositor, format!("focus-window {full}"));
    let member = window(&compositor, full);
    let x = member["x"].as_f64().unwrap() + 30.0;
    let y = member["y"].as_f64().unwrap() + 60.0;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 272");
    client.request_fullscreen(1);
    action(&compositor, "pointer-motion-abs 300 300");
    action(&compositor, "pointer-button-up 272");
    assert!(
        client.released_buttons().is_empty(),
        "a compositor-owned press must not leak its release into the fullscreen client"
    );
    click(&compositor, x, y);
    assert!(client.press_serial() > 0);
    assert_eq!(client.released_buttons(), &[272]);
    assert_eq!(group(&compositor)["selected"], json!([]));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
