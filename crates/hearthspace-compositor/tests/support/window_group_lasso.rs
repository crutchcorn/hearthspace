//! One-shot shape selection and direct manipulation of the selected windows.
use super::super::{interactive::client::InteractiveClient, navigation::wait_windows};
use super::*;

fn fixture() -> (HeadlessCompositor, InteractiveClient, [u64; 2]) {
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let client = InteractiveClient::connect(&socket);
    let windows = wait_windows(&compositor, |windows| windows.len() == 2);
    let ids = [0, 1].map(|number| {
        windows
            .iter()
            .find(|window| window["title"] == format!("Interactive fixture {number}"))
            .unwrap()["id"]
            .as_u64()
            .unwrap()
    });
    for (id, x) in [(ids[0], 100), (ids[1], 600)] {
        action(&compositor, format!("move-window {id} {x} 150"));
    }
    (compositor, client, ids)
}

fn drag_member(compositor: &HeadlessCompositor, id: u64, dx: f64, dy: f64) {
    let member = window(compositor, id);
    let x = member["x"].as_f64().unwrap() + 40.0;
    let y = member["y"].as_f64().unwrap() + 60.0;
    action(compositor, format!("pointer-motion-abs {x} {y}"));
    action(compositor, "pointer-button-down 272");
    action(
        compositor,
        format!("pointer-motion-abs {} {}", x + dx, y + dy),
    );
    action(compositor, "pointer-button-up 272");
}

#[test]
fn lasso_finishes_empty_gestures_and_replaces_the_previous_group() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, [first, second]) = fixture();
    for points in [&[(90.0, 140.0)][..], &[(90.0, 140.0), (900.0, 140.0)][..]] {
        select_by_keyboard(&compositor, first);
        action(&compositor, "group-selection-toggle");
        draw_lasso(&compositor, points);
        assert_eq!(group(&compositor)["selecting"], false);
        assert_eq!(group(&compositor)["selected"], json!([]));
    }
    select_by_keyboard(&compositor, first);
    action(&compositor, "group-selection-toggle");
    draw_lasso(
        &compositor,
        &[
            (580.0, 130.0),
            (870.0, 130.0),
            (870.0, 340.0),
            (580.0, 340.0),
        ],
    );
    assert_eq!(group(&compositor)["selected"], json!([second]));
    assert_eq!(group(&compositor)["selecting"], false);
    assert!(
        !client.received_pointer_press(),
        "lasso presses belong to the compositor"
    );
    assert!(client.released_buttons().is_empty());
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn selected_client_content_drags_the_group_and_outside_clicks_reach_the_client() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, [first, second]) = fixture();
    action(&compositor, "group-selection-toggle");
    draw_lasso(
        &compositor,
        &[(80.0, 130.0), (880.0, 130.0), (880.0, 350.0), (80.0, 350.0)],
    );
    assert_eq!(group(&compositor)["selected"], json!([first, second]));
    let button = group(&compositor)["gather_button"].clone();
    assert_eq!(
        button["width"], button["height"],
        "standalone square action"
    );
    assert!(
        button["x"].as_i64().unwrap() > 840,
        "action sits beside the windows"
    );
    drag_member(&compositor, first, 55.0, 35.0);
    drag_member(&compositor, second, -20.0, 40.0);
    for (id, old_x) in [(first, 100), (second, 600)] {
        let member = window(&compositor, id);
        assert_eq!(member["canvas_x"], old_x + 35);
        assert_eq!(member["canvas_y"], 225);
        assert_eq!(member["content_width"], 240);
        assert_eq!(member["content_height"], 160);
    }
    click(&compositor, 180.0, 280.0);
    assert_eq!(group(&compositor)["selected"], json!([first, second]));
    assert!(
        !client.received_pointer_press(),
        "selected content must not receive a group drag or click"
    );
    assert!(client.released_buttons().is_empty());
    // Empty space between members is outside the group, even though it lies
    // inside the group's overall bounds.
    click(&compositor, 500.0, 280.0);
    assert_eq!(group(&compositor)["selected"], json!([]));
    assert_eq!(group(&compositor)["gather_button"], Value::Null);
    select_by_keyboard(&compositor, first);
    click(&compositor, 680.0, 280.0);
    assert_eq!(group(&compositor)["selected"], json!([]));
    assert!(client.received_pointer_press());
    assert_eq!(client.released_buttons(), &[272]);
    select_by_keyboard(&compositor, first);
    action(&compositor, "pointer-motion-abs 680 280");
    action(&compositor, "pointer-button-down 273");
    action(&compositor, "pointer-button-up 273");
    assert_eq!(group(&compositor)["selected"], json!([]));
    assert_eq!(client.released_buttons(), &[272, 273]);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn selection_refuses_an_existing_client_move_without_stealing_its_grab() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, [first, second]) = fixture();
    action(&compositor, "pointer-motion-abs 140 210");
    action(&compositor, "pointer-button-down 272");
    let serial = client.press_serial();
    client.request_move(0, serial);
    assert!(
        compositor
            .send_text_command("group-selection-toggle")
            .starts_with("err ")
    );
    action(&compositor, "pointer-motion-abs 190 240");
    action(&compositor, "pointer-button-up 272");
    client.sync();
    assert_eq!(window(&compositor, first)["canvas_x"], 150);
    assert_eq!(window(&compositor, first)["canvas_y"], 180);
    assert_eq!(window(&compositor, second)["canvas_x"], 600);
    assert_eq!(client.released_buttons(), &[272]);
    assert_eq!(group(&compositor)["selected"], json!([]));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
