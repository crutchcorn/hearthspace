//! Group movement must preserve usable X11 input after releasing selection.
use super::*;
use crate::navigation::keys;
use serde_json::json;

fn group_move_then_csd(start: [i32; 2], destination: [i32; 2]) {
    let _guard = headless_test_guard();
    let mut scene = Scene::new(destination);
    let id = scene.client.id;
    // Keep Scene's panned view and neighbor fixed. Both the group drag's start
    // and destination fit on screen, including the positive-to-negative move.
    action(
        &scene.compositor,
        format!("move-window {id} {} {}", start[0], start[1]),
    );
    scene.client.barrier(&scene.compositor);
    scene.placed(start);
    scene.unchanged();
    let windows = query(&scene.compositor, "windows");
    let focused = windows["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == id)
        .unwrap();
    assert_eq!(focused["focused"], true);
    // Do not call focus-window here: its reveal behavior could change the
    // viewport and conceal a regression in the group movement path.
    action(&scene.compositor, "group-selection-toggle");
    keys(&scene.compositor, &[57]); // Space selects the already focused client.
    keys(&scene.compositor, &[28]); // Enter retains the selected group.
    let group = query(&scene.compositor, "group-status");
    assert_eq!(group["selected"], json!([id]));
    assert_eq!(group["selecting"], false);

    scene.client.clear_events();
    scene.move_input(false, scene.screen(start, [134, 80]));
    action(&scene.compositor, "pointer-button-down 272");
    scene.move_input(false, scene.screen(destination, [134, 80]));
    scene.placed(destination);
    scene.unchanged();
    action(&scene.compositor, "pointer-button-up 272");
    scene.move_input(false, [1100.0, 650.0]);
    scene.client.barrier(&scene.compositor);
    assert!(scene.client.presses.is_empty(), "group press reached X11");
    assert!(
        scene.client.releases.is_empty(),
        "group release reached X11"
    );
    assert_eq!(
        query(&scene.compositor, "group-status")["selected"],
        json!([id]),
        "release must retain selection until explicitly cleared"
    );
    action(&scene.compositor, "group-clear");
    let cleared = query(&scene.compositor, "group-status");
    assert_eq!(cleared["selected"], json!([]));
    assert_eq!(cleared["selecting"], false);
    scene.placed(destination);
    scene.unchanged();

    // Query and press the actual titlebar after group cleanup. Checking only
    // the next move would miss clipping: its configure could repair the root.
    scene.move_input(false, scene.screen(destination, [134, 17]));
    scene.client.clear_events();
    action(&scene.compositor, "pointer-button-down 272");
    let press = scene.client.wait_press();
    assert_eq!(press.detail, 1);
    assert_eq!(scene.client.presses.len(), 1);
    assert_local([press.event_x, press.event_y], [134, 17]);
    let pointer = scene
        .client
        .connection
        .query_pointer(scene.client.window)
        .unwrap()
        .reply()
        .unwrap();
    assert!(pointer.same_screen);
    assert_local([pointer.win_x, pointer.win_y], [134, 17]);
    scene.placed(destination);
    scene.unchanged();

    scene
        .client
        .request(&scene.compositor, 8, 1, (press.root_x, press.root_y));
    scene.client.clear_events();
    scene.move_input(false, scene.screen(destination, [234, 67]));
    let moved = [destination[0] + 100, destination[1] + 50];
    scene.placed(moved);
    scene.client.barrier(&scene.compositor);
    assert_eq!(scene.client.motions, 0, "CSD drag leaked into the client");
    action(&scene.compositor, "pointer-button-up 272");
    scene.move_input(false, [1100.0, 650.0]);
    scene.client.barrier(&scene.compositor);
    scene.placed(moved);
    scene.unchanged();
    scene.close(moved);
}

#[test]
fn x11_group_move_across_negative_root_boundary_preserves_csd_move_and_close() {
    group_move_then_csd([89, 32], [89, -179]);
}

#[test]
fn x11_group_move_at_far_positive_canvas_preserves_csd_move_and_close() {
    group_move_then_csd([89_900, 69_950], [90_000, 70_000]);
}
