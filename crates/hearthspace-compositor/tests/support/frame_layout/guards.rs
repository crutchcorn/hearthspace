//! Deferred shell actions and new presses respect the current input owner.
use super::super::{canvas_map_client::MapClient, shell_panels::click};
use super::*;
use hearthspace_common::frame_layout::{
    frame_widget_app_id, layout_confirm_app_id, layout_editor_app_id,
};
use std::os::unix::net::UnixDatagram;

fn reject_queued_actions(
    compositor: &HeadlessCompositor,
    client: &mut MapClient,
    commands: &[String],
) {
    let focused = client.focus();
    let before = layout(compositor);
    for command in commands {
        assert!(
            compositor.send_text_command(command).starts_with("err "),
            "a deferred frame action bypassed the active input mode: {command}"
        );
    }
    let after = layout(compositor);
    assert_eq!(after["pinned_windows"], before["pinned_windows"]);
    assert_eq!(after["focus_request"], before["focus_request"]);
    assert_eq!(client.focus(), focused);
}

#[test]
fn frame_layout_queued_external_actions_preserve_editor_modal_search_and_switcher_focus() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut client = connect(&compositor);
    client.add("frame-action-owner".into(), (500, 320), (0, 0), COLOR);
    let id = window(&compositor, "frame-action-owner")["id"]
        .as_u64()
        .unwrap();
    pin(&compositor, id);
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let clock = client.add(
        frame_widget_app_id(Some(&output), "clock"),
        (94, 44),
        (0, 0),
        COLOR,
    );
    let wifi = client.add(
        frame_widget_app_id(Some(&output), "wifi"),
        (44, 44),
        (0, 0),
        COLOR,
    );
    let next = format!(
        "frame-focus-next {}",
        json!({"output":output,"target":{"kind":"widget","id":"clock"},"reverse":false})
    );
    action(&compositor, &next);
    assert_eq!(client.focus(), Some(wifi));
    action(
        &compositor,
        format!(
            "frame-focus-next {}",
            json!({"output":output,"target":{"kind":"widget","id":"wifi"},"reverse":true})
        ),
    );
    assert_eq!(client.focus(), Some(clock));
    let queued = [format!("frame-reveal {id}"), next];

    action(&compositor, "layout-tool");
    let opened = layout(&compositor);
    let editor = client.add(
        layout_editor_app_id(opened["session"].as_u64().unwrap(), Some(&output)),
        (260, 156),
        (0, 0),
        COLOR,
    );
    assert_eq!(client.focus(), Some(editor));
    reject_queued_actions(&compositor, &mut client, &queued);
    command(
        &compositor,
        "remove",
        json!({"kind":"widget","id":"tools"}),
        Value::Null,
    );
    let modal = client.add(
        layout_confirm_app_id(opened["session"].as_u64().unwrap(), Some(&output)),
        (420, 220),
        (0, 0),
        COLOR,
    );
    assert_eq!(client.focus(), Some(modal));
    reject_queued_actions(&compositor, &mut client, &queued);
    command(&compositor, "cancel", Value::Null, Value::Null);
    command(&compositor, "done", Value::Null, Value::Null);
    client.destroy(modal);
    client.destroy(editor);

    action(&compositor, "canvas-search-open");
    let search = query(&compositor, "canvas-search-status");
    let session = search["session"].as_u64().unwrap();
    let field = client.add(
        hearthspace_common::canvas_search_app_id(session, search["output"].as_str()),
        (420, 54),
        (0, 0),
        COLOR,
    );
    assert_eq!(client.focus(), Some(field));
    reject_queued_actions(&compositor, &mut client, &queued);
    action(&compositor, format!("canvas-search-close {session}"));
    client.destroy(field);

    action(&compositor, "key-down 56");
    keys(&compositor, &[15]);
    assert_eq!(query(&compositor, "window-switcher-status")["active"], true);
    reject_queued_actions(&compositor, &mut client, &queued);
    keys(&compositor, &[1]);
    action(&compositor, "key-up 56");
    action(&compositor, &queued[0]);
    assert_eq!(layout(&compositor)["pinned_windows"][0]["expanded"], true);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_pinned_item_preserves_an_existing_client_grab_and_activates_after_release() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut client = connect(&compositor);
    let normal = client.add("frame-grab-owner".into(), (500, 320), (0, 0), COLOR);
    let id = "file:50726f6a65637473";
    let item = client.add(
        hearthspace_common::desktop_item_app_id(id),
        (100, 108),
        (0, 0),
        COLOR,
    );
    action(&compositor, "layout-tool");
    command(
        &compositor,
        "place",
        json!({"kind":"item","id":id}),
        json!({"surface":"frame","edge":"right","position":0.5}),
    );
    client.resize(item, (54, 54));
    command(&compositor, "done", Value::Null, Value::Null);
    let events = UnixDatagram::bind(hearthspace_common::shell_event_socket_path(
        &compositor.command_socket_path,
    ))
    .unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let bounds = window(&compositor, "frame-grab-owner");
    let x = bounds["x"].as_f64().unwrap() + 100.;
    let y = bounds["y"].as_f64().unwrap() + 100.;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 273");
    // The standalone54px item is24px from the right edge and vertically centered.
    action(&compositor, "pointer-motion-abs 1229 400");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "pointer-button-up 273");
    assert_eq!(client.presses().len(), 2);
    assert_eq!(client.releases().len(), 2);
    assert!(client.presses().iter().all(|press| press.0 == normal));
    assert!(client.releases().iter().all(|release| release.0 == normal));

    click(&compositor, 1229., 400.);
    let event = super::super::shell_events::receive(&events, "desktop-action");
    let accepted = query(
        &compositor,
        &format!("desktop-event-accept {}", event["token"].as_u64().unwrap()),
    );
    assert_eq!(accepted["action"], "activate");
    assert_eq!(accepted["id"], id);
    assert_eq!(client.presses().len(), 2);
    assert_eq!(client.releases().len(), 2);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
