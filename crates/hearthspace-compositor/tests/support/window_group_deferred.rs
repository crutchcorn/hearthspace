//! Completed inference cannot seize geometry from an interactive move/resize.
use super::super::{interactive::client::InteractiveClient, navigation::wait_windows};
use super::*;

#[test]
fn gather_completion_waits_for_group_drag_and_clear_discards_deferred_results() {
    let _guard = headless_test_guard();
    let server = mock::MockOllama::start();
    let mut compositor =
        HeadlessCompositor::spawn_with_command("1280x800", None, None, 1, false, |command| {
            command.env("HEARTHSPACE_OLLAMA_URL", &server.url);
        });
    drop(compositor.wait_for_socket());
    let mut editors = Editors(Vec::new());
    let (seed, _) = editor(&compositor, &mut editors, 0);
    let (candidate, _) = editor(&compositor, &mut editors, 1);
    for cancel in [true, false] {
        select_by_keyboard(&compositor, seed);
        action(&compositor, format!("move-window {candidate} 9000 9000"));
        action(&compositor, "group-gather");
        let request = server.request(&compositor);
        assert_eq!(
            request["state"]["window_titles"],
            json!(["Overview editor 0"])
        );
        assert_eq!(
            request["questions"]["relevant"]["instructions"]["candidate_window_title"],
            "Overview editor 1"
        );
        let member = window(&compositor, seed);
        let x = member["x"].as_f64().unwrap() + 30.0;
        let y = member["y"].as_f64().unwrap() + 60.0;
        action(&compositor, format!("pointer-motion-abs {x} {y}"));
        action(&compositor, "pointer-button-down 272");
        action(
            &compositor,
            format!("pointer-motion-abs {} {}", x + 70.0, y + 35.0),
        );
        server.reply(0.99);
        let deferred = wait_group(&compositor, |status| status["waiting_for_drag"] == true);
        assert_eq!(deferred["gathering"], true);
        assert_eq!(deferred["selected"], json!([seed]));
        assert_eq!(window(&compositor, candidate)["canvas_x"], 9000);
        action(
            &compositor,
            format!("pointer-motion-abs {} {}", x + 90.0, y + 45.0),
        );
        if cancel {
            keys(&compositor, &[1]);
        }
        action(&compositor, "pointer-button-up 272");
        let completed = wait_group(&compositor, |status| {
            status["gathering"] == false && status["animating"] == false
        });
        if cancel {
            assert_eq!(completed["selected"], json!([]));
            assert_eq!(window(&compositor, candidate)["canvas_x"], 9000);
        } else {
            assert_eq!(completed["selected"], json!([seed, candidate]));
            assert_eq!(window(&compositor, candidate)["visible"], true);
        }
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn outside_client_move_or_resize_cancels_gather_without_stealing_client_input() {
    let _guard = headless_test_guard();
    let server = mock::MockOllama::start();
    let mut compositor =
        HeadlessCompositor::spawn_with_command("1280x800", None, None, 1, false, |command| {
            command.env("HEARTHSPACE_OLLAMA_URL", &server.url);
        });
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
    let (seed, candidate) = (id(0), id(1));
    for resize in [false, true] {
        action(&compositor, format!("move-window {seed} 80 150"));
        action(&compositor, format!("move-window {candidate} 650 350"));
        select_by_keyboard(&compositor, seed);
        action(&compositor, "group-gather");
        server.request(&compositor);
        action(&compositor, "pointer-motion-abs 680 400");
        action(&compositor, "pointer-button-down 272");
        let serial = client.press_serial();
        if resize {
            client.request_resize(1, serial);
        } else {
            client.request_move(1, serial);
        }
        server.reply(0.99);
        let cleared = group(&compositor);
        assert_eq!(cleared["selected"], json!([]));
        assert_eq!(cleared["gathering"], false);
        action(&compositor, "pointer-motion-abs 720 420");
        client.sync();
        assert_eq!(group(&compositor)["selected"], json!([]));
        let moving = window(&compositor, candidate);
        assert_eq!(moving["canvas_x"], if resize { 650 } else { 690 });
        assert_eq!(moving["canvas_y"], if resize { 350 } else { 370 });
        if resize {
            assert_eq!(moving["content_width"], 280);
        }
        action(&compositor, "pointer-button-up 272");
        client.sync();
        let completed = wait_group(&compositor, |status| {
            status["gathering"] == false && status["animating"] == false
        });
        assert_eq!(completed["selected"], json!([]));
        server.no_request();
        assert_eq!(
            window(&compositor, candidate)["canvas_x"],
            if resize { 650 } else { 690 }
        );
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
