//! A late-mapped grip must not intercept presses on a group's real controls.
use super::*;
use hearthspace_common::frame_layout::{
    frame_group_app_id, frame_group_grip_app_id, frame_widget_app_id,
};

fn center(rect: &Value) -> (f64, f64) {
    (
        rect["x"].as_f64().unwrap() + rect["width"].as_f64().unwrap() / 2.,
        rect["y"].as_f64().unwrap() + rect["height"].as_f64().unwrap() / 2.,
    )
}

fn widget(state: &Value, id: &str) -> Value {
    state["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|widget| widget["id"] == id)
        .unwrap()
        .clone()
}

fn motion(compositor: &HeadlessCompositor, touch: bool, point: (f64, f64)) {
    let command = if touch {
        "touch-motion member-hit 0"
    } else {
        "pointer-motion-abs"
    };
    action(compositor, format!("{command} {} {}", point.0, point.1));
    if touch {
        action(compositor, "touch-frame member-hit");
    }
}

fn press(compositor: &HeadlessCompositor, touch: bool, point: (f64, f64)) {
    if touch {
        action(
            compositor,
            format!("touch-down member-hit 0 {} {}", point.0, point.1),
        );
        action(compositor, "touch-frame member-hit");
        // A contact starts as hover. Cross the drag threshold inside the
        // original target so selection is exercised before the large move.
        motion(compositor, true, (point.0 + 9., point.1));
    } else {
        motion(compositor, false, point);
        action(compositor, "pointer-button-down 272");
    }
}

fn release(compositor: &HeadlessCompositor, touch: bool) {
    action(
        compositor,
        if touch {
            "touch-up member-hit 0"
        } else {
            "pointer-button-up 272"
        },
    );
    if touch {
        action(compositor, "touch-frame member-hit");
    }
}

#[rstest::rstest]
#[case::mouse(false)]
#[case::touch(true)]
fn frame_layout_late_group_grip_preserves_member_detach_and_regroup(#[case] touch: bool) {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    for (id, size) in [("clock", (70, 44)), ("wifi", (44, 44))] {
        client.add(frame_widget_app_id(Some(&output), id), size, (0, 0), COLOR);
    }
    client.add(
        frame_group_app_id(Some(&output), "system"),
        (320, 54),
        (0, 0),
        COLOR,
    );
    // The real shell maps group material and its grip after member windows.
    client.add(
        frame_group_grip_app_id(Some(&output), "system"),
        (28, 24),
        (0, 0),
        COLOR,
    );
    action(&compositor, "layout-tool");
    let before = layout(&compositor);
    press(&compositor, touch, center(&widget(&before, "wifi")["rect"]));
    assert_eq!(
        layout(&compositor)["selected"]["target"],
        json!({"kind":"widget","id":"wifi"}),
        "the group grip intercepted a member press"
    );
    motion(&compositor, touch, (400., 350.));
    release(&compositor, touch);
    let detached = layout(&compositor);
    assert!(widget(&detached, "wifi")["group"].is_null());
    assert_eq!(
        detached["outputs"][0]["layout"]["placements"]["widget:wifi"]["surface"],
        "canvas"
    );
    assert_eq!(
        detached["outputs"][0]["layout"]["placements"]["group:system"],
        before["outputs"][0]["layout"]["placements"]["group:system"]
    );
    press(
        &compositor,
        touch,
        center(&widget(&detached, "wifi")["rect"]),
    );
    let destination = center(&widget(&detached, "clock")["rect"]);
    motion(&compositor, touch, destination);
    release(&compositor, touch);
    let regrouped = layout(&compositor);
    assert_eq!(widget(&regrouped, "wifi")["group"], "system");
    assert!(regrouped["outputs"][0]["layout"]["placements"]["widget:wifi"].is_null());

    let grip = &regrouped["outputs"][0]["groups"][0]["grip"];
    press(&compositor, touch, center(grip));
    assert_eq!(
        layout(&compositor)["selected"]["target"],
        json!({"kind":"group","id":"system"}),
        "the actual grip must still select the group"
    );
    motion(&compositor, touch, (650., 450.));
    keys(&compositor, &[1]);
    release(&compositor, touch);
    assert_eq!(
        layout(&compositor)["outputs"][0]["layout"],
        regrouped["outputs"][0]["layout"]
    );
    assert!(client.presses().is_empty());
    assert!(client.releases().is_empty());
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
