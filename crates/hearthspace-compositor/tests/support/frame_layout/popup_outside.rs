//! Real outside presses cancel anchors; the initiating bar and panel do not.
use super::super::shell_panels::click;
use super::*;
use hearthspace_common::frame_layout::frame_widget_app_id;
use std::os::unix::net::UnixDatagram;

fn epoch(compositor: &HeadlessCompositor) -> u64 {
    layout(compositor)["popup_epoch"].as_u64().unwrap()
}

fn anchor(compositor: &HeadlessCompositor, output: &str) {
    action(
        compositor,
        format!(
            "frame-popup-anchor {}",
            json!({
                "app_id":hearthspace_common::notifications_app_id_for_output(output),
                "output":output,"target":{"kind":"widget","id":"notifications"}
            })
        ),
    );
}

fn widget_point(compositor: &HeadlessCompositor) -> (f64, f64) {
    let snapshot = layout(compositor);
    let rect = &snapshot["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|widget| widget["id"] == "notifications")
        .unwrap()["rect"];
    (
        rect["x"].as_f64().unwrap() + 22.,
        rect["y"].as_f64().unwrap() + 22.,
    )
}

#[test]
fn frame_popup_outside_press_revokes_anchor_but_initiating_bar_release_does_not() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let normal = client.add("outside-anchor-test".into(), (300, 200), (0, 0), COLOR);
    let widget = client.add(
        frame_widget_app_id(Some(&output), "notifications"),
        (44, 44),
        (0, 0),
        COLOR,
    );
    let (x, y) = widget_point(&compositor);
    let before = epoch(&compositor);
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 272");
    assert_eq!(client.focus(), Some(widget));
    assert_eq!(epoch(&compositor), before);
    anchor(&compositor, &output);
    action(&compositor, "pointer-button-up 272");
    assert_eq!(
        epoch(&compositor),
        before,
        "the initiating release cannot cancel its anchor"
    );

    let events = UnixDatagram::bind(hearthspace_common::shell_event_socket_path(
        &compositor.command_socket_path,
    ))
    .unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let bounds = window(&compositor, "outside-anchor-test");
    let x = bounds["x"].as_f64().unwrap() + 100.;
    let y = bounds["y"].as_f64().unwrap() + 100.;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    assert_eq!(epoch(&compositor), before, "hover alone must not cancel");
    action(&compositor, "pointer-button-down 272");
    let cancelled = epoch(&compositor);
    assert!(cancelled > before);
    let event = super::super::shell_events::receive(&events, "cancel-shell-popups");
    assert_eq!(event["popup_epoch"], cancelled);
    assert_eq!(event["dismiss"], true);
    action(&compositor, "pointer-button-up 272");
    assert_eq!(epoch(&compositor), cancelled);
    assert_eq!(client.focus(), Some(normal));
    assert!(
        query(&compositor, "status")["shell_panels"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(client.closed().is_empty());
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_popup_mapping_inside_input_and_bar_focus_preserve_the_anchor_epoch() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let widget = client.add(
        frame_widget_app_id(Some(&output), "notifications"),
        (44, 44),
        (0, 0),
        COLOR,
    );
    anchor(&compositor, &output);
    let before = epoch(&compositor);
    let panel = client.add(
        hearthspace_common::notifications_app_id_for_output(&output),
        (240, 160),
        (0, 0),
        COLOR,
    );
    assert_eq!(client.focus(), Some(panel));
    assert_eq!(
        epoch(&compositor),
        before,
        "mapping focus must not cancel the new popup"
    );
    let status = query(&compositor, "status");
    let rect = &status["shell_panels"][0];
    click(
        &compositor,
        rect["x"].as_f64().unwrap() + 30.,
        rect["y"].as_f64().unwrap() + 30.,
    );
    assert_eq!(client.focus(), Some(panel));
    assert_eq!(epoch(&compositor), before);
    let (x, y) = widget_point(&compositor);
    click(&compositor, x, y);
    assert_eq!(client.focus(), Some(widget));
    assert_eq!(epoch(&compositor), before);
    assert!(!client.closed().contains(&panel));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
