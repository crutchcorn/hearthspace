//! Recovery must see takeovers even when no shell is listening for wakeups.
use super::super::session_lock::lock_client::LockClient;
use super::*;

fn epoch(compositor: &HeadlessCompositor) -> u64 {
    layout(compositor)["popup_epoch"].as_u64().unwrap()
}

#[test]
fn frame_popup_epoch_persists_after_editor_confirmation_and_lock_takeovers() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    // Deliberately leave the shell event socket unbound: cancellation must not
    // depend on successful datagram delivery or observing a modal snapshot.
    let before = epoch(&compositor);
    action(&compositor, "layout-tool");
    command(&compositor, "done", Value::Null, Value::Null);
    let after_editor = epoch(&compositor);
    assert!(after_editor > before);
    assert_eq!(layout(&compositor)["editing"], false);

    action(&compositor, "layout-tool");
    let before_confirmation = epoch(&compositor);
    command(
        &compositor,
        "remove",
        json!({"kind":"widget","id":"tools"}),
        Value::Null,
    );
    command(&compositor, "cancel", Value::Null, Value::Null);
    command(&compositor, "done", Value::Null, Value::Null);
    let after_confirmation = epoch(&compositor);
    assert!(after_confirmation > before_confirmation);
    assert!(layout(&compositor)["confirmation"].is_null());

    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = LockClient::connect(&socket);
    locker.wait_locked();
    let locked = layout(&compositor);
    assert!(locked["outputs"].as_array().unwrap().is_empty());
    assert!(locked["popup_epoch"].as_u64().unwrap() > after_confirmation);
    locker.unlock();
    let recovered = layout(&compositor);
    assert!(!recovered["outputs"].as_array().unwrap().is_empty());
    assert!(recovered["popup_epoch"].as_u64().unwrap() > after_confirmation);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_popup_escape_invalidates_an_acknowledged_anchor_without_a_mapped_panel() {
    use hearthspace_common::frame_layout::frame_widget_app_id;
    use std::os::unix::net::UnixDatagram;
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
    keys(&compositor, &[125, 64]);
    assert_eq!(client.focus(), Some(widget));
    let before = epoch(&compositor);
    action(
        &compositor,
        format!(
            "frame-popup-anchor {}",
            json!({
                "app_id":hearthspace_common::notifications_app_id_for_output(&output),
                "output":output,"target":{"kind":"widget","id":"notifications"}
            })
        ),
    );
    let events = UnixDatagram::bind(hearthspace_common::shell_event_socket_path(
        &compositor.command_socket_path,
    ))
    .unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    keys(&compositor, &[1]);
    let event = super::super::shell_events::receive(&events, "cancel-shell-popups");
    assert!(event["popup_epoch"].as_u64().unwrap() > before);
    assert_eq!(layout(&compositor)["popup_epoch"], event["popup_epoch"]);
    assert!(
        client.closed().is_empty(),
        "the persistent widget must remain mapped"
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
