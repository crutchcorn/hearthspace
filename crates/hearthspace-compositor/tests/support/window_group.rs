use super::{
    navigation::{Editors, action, keys, query},
    overview::{editor, wait_state},
    *,
};
use serde_json::{Value, json};

#[path = "window_group_deferred.rs"]
mod deferred;
#[path = "window_group_lasso.rs"]
mod lasso;
#[path = "window_group_mock.rs"]
mod mock;
#[path = "window_group_visibility.rs"]
mod visibility;

fn group(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "group-status")
}

fn window(compositor: &HeadlessCompositor, id: u64) -> Value {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["id"] == id)
        .unwrap()
        .clone()
}

fn click(compositor: &HeadlessCompositor, x: f64, y: f64) {
    action(compositor, format!("pointer-motion-abs {x} {y}"));
    action(compositor, "pointer-button-down 272");
    action(compositor, "pointer-button-up 272");
}

fn select_by_keyboard(compositor: &HeadlessCompositor, id: u64) {
    action(compositor, format!("focus-window {id}"));
    action(compositor, "group-selection-toggle");
    keys(compositor, &[57]); // Space toggles the focused window.
    keys(compositor, &[28]); // Enter finishes selection, retaining the group.
    assert_eq!(group(compositor)["selected"], json!([id]));
}

fn wait_group(compositor: &HeadlessCompositor, predicate: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = group(compositor);
        if predicate(&status) {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "group condition timed out: {status}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn draw_lasso(compositor: &HeadlessCompositor, points: &[(f64, f64)]) {
    let &(x, y) = points.first().unwrap();
    action(compositor, format!("pointer-motion-abs {x} {y}"));
    action(compositor, "pointer-button-down 272");
    for &(x, y) in &points[1..] {
        action(compositor, format!("pointer-motion-abs {x} {y}"));
    }
    action(compositor, "pointer-button-up 272");
}

#[test]
fn window_group_concave_lasso_moves_members_at_zoom_and_clears_on_lock() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let (first, _) = editor(&compositor, &mut editors, 0);
    let (second, _) = editor(&compositor, &mut editors, 1);
    let (third, third_state) = editor(&compositor, &mut editors, 2);
    for (id, x, y) in [(first, 100, 200), (second, 800, 200), (third, 440, 370)] {
        action(&compositor, format!("resize-window {id} 240 190"));
        action(&compositor, format!("move-window {id} {x} {y}"));
    }
    action(&compositor, "pan-to 0 0");
    action(&compositor, "zoom-to 0.8");
    let deadline = Instant::now() + Duration::from_secs(5);
    while query(&compositor, "status")["viewport"]["animating"] == true {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    }
    action(&compositor, "pan-to 0 0");
    action(&compositor, "group-selection-toggle");
    keys(&compositor, &[30]); // Armed selection must not type A in the editor.
    // Concave shape surrounds left/right windows but excludes the third even
    // though it lies entirely inside the lasso's bounding rectangle.
    let points = [
        (60.0, 140.0),
        (1100.0, 140.0),
        (1100.0, 680.0),
        (740.0, 680.0),
        (740.0, 280.0),
        (380.0, 280.0),
        (380.0, 680.0),
        (60.0, 680.0),
    ]
    .map(|(x, y)| (x * 0.8, y * 0.8));
    draw_lasso(&compositor, &points);
    assert_eq!(group(&compositor)["selected"], json!([first, second]));
    assert_eq!(
        group(&compositor)["selecting"],
        false,
        "release finishes selection"
    );
    keys(&compositor, &[48]); // No Enter is needed before normal keyboard input.
    wait_state(&third_state, |state| state["text"] == "b");
    let before = [window(&compositor, first), window(&compositor, second)];
    let x = before[0]["x"].as_f64().unwrap() + 30.0;
    let y = before[0]["y"].as_f64().unwrap() + 60.0;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 272");
    action(
        &compositor,
        format!("pointer-motion-abs {} {}", x + 64.0, y + 48.0),
    );
    action(&compositor, "axis 0 -120");
    action(&compositor, "pointer-button-up 272");
    for (old, id) in before.iter().zip([first, second]) {
        let new = window(&compositor, id);
        assert_eq!(
            new["canvas_x"].as_i64().unwrap() - old["canvas_x"].as_i64().unwrap(),
            80
        );
        assert_eq!(
            new["canvas_y"].as_i64().unwrap() - old["canvas_y"].as_i64().unwrap(),
            60
        );
    }
    assert_eq!(window(&compositor, third)["canvas_x"], 440);
    assert_eq!(window(&compositor, third)["canvas_y"], 370);
    assert_eq!(query(&compositor, "status")["viewport"]["scale"], 0.8);
    editors.0[1].kill().unwrap();
    editors.0[1].wait().unwrap();
    wait_group(&compositor, |status| status["selected"] == json!([first]));
    let item = window(&compositor, first);
    action(
        &compositor,
        format!(
            "pointer-motion-abs {} {}",
            item["x"].as_f64().unwrap() + 30.0,
            item["y"].as_f64().unwrap() + 60.0
        ),
    );
    action(&compositor, "pointer-button-down 272");
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = super::session_lock::lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    assert!(query(&compositor, "status").get("window_group").is_none());
    assert!(
        compositor
            .send_text_command("group-status")
            .starts_with("err session is locked")
    );
    locker.unlock();
    assert_eq!(group(&compositor)["selected"], json!([]));
    // Losing the captured release at lock must not eat the next client's up.
    click(
        &compositor,
        item["x"].as_f64().unwrap() + 50.0,
        item["y"].as_f64().unwrap() + 80.0,
    );
    action(&compositor, "group-selection-toggle");
    keys(&compositor, &[1]);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn window_group_gathers_asynchronously_from_fixed_titles_and_restores_pinned_matches() {
    let _guard = headless_test_guard();
    let server = mock::MockOllama::start();
    let mut compositor =
        HeadlessCompositor::spawn_with_command("1280x800", None, None, 1, false, |command| {
            command.env("HEARTHSPACE_OLLAMA_URL", &server.url);
        });
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let mut ids = Vec::new();
    for number in 0..5 {
        ids.push(editor(&compositor, &mut editors, number).0);
    }
    select_by_keyboard(&compositor, ids[0]);
    action(&compositor, format!("move-window {} 9000 -5000", ids[1]));
    action(&compositor, format!("move-window {} -8000 8000", ids[2]));
    action(&compositor, format!("maximize-window {} true", ids[3]));
    action(&compositor, format!("fullscreen-window {} true", ids[4]));
    action(&compositor, "group-gather");
    let first = server.request(&compositor);
    assert_eq!(group(&compositor)["gathering"], true);
    // A delayed HTTP response cannot block the compositor's control/input loop.
    let started = Instant::now();
    action(&compositor, "pointer-motion-abs 10 10");
    assert!(started.elapsed() < Duration::from_secs(1));
    for (index, request) in std::iter::once(first)
        .chain((0..3).map(|_| server.request(&compositor)))
        .enumerate()
    {
        assert_eq!(
            request["state"]["window_titles"],
            json!(["Overview editor 0"])
        );
        assert_eq!(
            request["questions"]["relevant"]["instructions"]["candidate_window_title"],
            format!("Overview editor {}", index + 1)
        );
        server.reply(if index == 1 { 0.05 } else { 0.98 });
    }
    let completed = wait_group(&compositor, |status| {
        status["gathering"] == false && status["animating"] == false
    });
    assert_eq!(
        completed["selected"],
        json!([ids[0], ids[1], ids[3], ids[4]])
    );
    assert_eq!(completed["error"], Value::Null);
    let mut positions = std::collections::BTreeSet::new();
    for id in [ids[1], ids[3], ids[4]] {
        let item = window(&compositor, id);
        assert_eq!(item["visible"], true, "{item}");
        assert_eq!(item["maximized"], false);
        assert_eq!(item["fullscreen"], false);
        assert!(positions.insert((
            item["canvas_x"].as_i64().unwrap(),
            item["canvas_y"].as_i64().unwrap()
        )));
    }
    assert_eq!(window(&compositor, ids[2])["canvas_x"], -8000);
    assert_eq!(window(&compositor, ids[2])["canvas_y"], 8000);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn window_group_escape_cancels_stale_results_and_unavailable_model_keeps_layout() {
    let _guard = headless_test_guard();
    let server = mock::MockOllama::start();
    let mut compositor =
        HeadlessCompositor::spawn_with_command("1280x800", None, None, 1, false, |command| {
            command.env("HEARTHSPACE_OLLAMA_URL", &server.url);
        });
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let (first, _) = editor(&compositor, &mut editors, 0);
    let (second, _) = editor(&compositor, &mut editors, 1);
    select_by_keyboard(&compositor, first);
    action(&compositor, format!("move-window {second} 9000 9000"));
    let button = group(&compositor)["gather_button"].clone();
    click(
        &compositor,
        button["x"].as_f64().unwrap() + 22.0,
        button["y"].as_f64().unwrap() + 22.0,
    );
    server.request(&compositor);
    action(&compositor, "pointer-motion-abs 10 10");
    let loading = compositor.take_screenshot();
    thread::sleep(Duration::from_millis(120));
    assert_ne!(
        loading,
        compositor.take_screenshot(),
        "loading shimmer must advance while the model is blocked"
    );
    keys(&compositor, &[1]);
    assert_eq!(group(&compositor)["selected"], json!([]));
    server.reply(1.0);
    server.no_request();
    assert_eq!(window(&compositor, second)["canvas_x"], 9000);
    assert_eq!(group(&compositor)["selected"], json!([]));
    drop(server);
    select_by_keyboard(&compositor, first);
    action(&compositor, "group-gather");
    let failed = wait_group(&compositor, |status| {
        status["gathering"] == false && status["error"].is_string()
    });
    assert_eq!(failed["selected"], json!([first]));
    assert_eq!(window(&compositor, second)["canvas_x"], 9000);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
