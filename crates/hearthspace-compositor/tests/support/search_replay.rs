//! Pending shell search input must survive activation races and focus changes.
use super::navigation::{action, query};
use super::shell_panels::{click, connect};
use super::*;
use serde_json::Value;
const GREEN: [u8; 4] = [0x66, 0xcc, 0, 0xff];

#[test]
fn repeated_search_activation_preserves_accepted_request_and_early_typing() {
    use std::{io::ErrorKind, os::unix::net::UnixDatagram};

    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let events = UnixDatagram::bind(hearthspace_common::shell_event_socket_path(
        &compositor.command_socket_path,
    ))
    .unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut client = connect(&compositor);
    let normal = client.add(
        "dev.hearthspace.RepeatedSearchTest".into(),
        (240, 160),
        (0, 0),
        GREEN,
    );
    client.add(
        hearthspace_common::SHELL_BAR_APP_ID.into(),
        (1280, 48),
        (0, 0),
        GREEN,
    );
    action(&compositor, "focus-launcher");
    let mut bytes = [0; 4096];
    let size = events.recv(&mut bytes).unwrap();
    let event: Value = serde_json::from_slice(&bytes[..size]).unwrap();
    let token = event["token"].as_u64().unwrap();
    action(&compositor, format!("shell-event-accept {token}"));
    super::navigation::keys(&compositor, &[30]);
    for _ in 0..4 {
        action(&compositor, "focus-launcher");
    }
    events.set_nonblocking(true).unwrap();
    assert_eq!(
        events.recv(&mut bytes).unwrap_err().kind(),
        ErrorKind::WouldBlock,
        "repeated activation must reuse the accepted in-flight request"
    );
    let search = client.add(
        hearthspace_common::launcher_app_id_for_request(None, token),
        (500, 300),
        (0, 0),
        GREEN,
    );
    assert_eq!(client.focus(), Some(search));
    assert!(!client.closed().contains(&search));
    assert_eq!(
        client.keys(),
        [30, 30],
        "coalescing must preserve buffered input"
    );
    action(&compositor, "focus-launcher");
    assert_eq!(client.focus(), Some(search));
    assert!(
        !client.closed().contains(&search),
        "explicit activation is idempotent"
    );
    super::navigation::keys(&compositor, &[125]);
    assert!(
        client.closed().contains(&search),
        "bare Meta still toggles mapped search closed"
    );
    client.destroy(search);
    assert_eq!(client.focus(), Some(normal));
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn search_replay_finishes_key_pairs_after_buffered_shortcut_changes_focus() {
    use std::os::unix::net::UnixDatagram;

    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let events = UnixDatagram::bind(hearthspace_common::shell_event_socket_path(
        &compositor.command_socket_path,
    ))
    .unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut client = connect(&compositor);
    let normal = client.add(
        "dev.hearthspace.ReplayTest".into(),
        (240, 160),
        (0, 0),
        GREEN,
    );
    client.add(
        hearthspace_common::SHELL_BAR_APP_ID.into(),
        (1280, 48),
        (0, 0),
        GREEN,
    );
    let id = query(&compositor, "windows")["windows"][0]["id"]
        .as_u64()
        .unwrap();
    action(&compositor, "focus-launcher");
    let mut bytes = [0; 4096];
    let size = events.recv(&mut bytes).unwrap();
    let event: Value = serde_json::from_slice(&bytes[..size]).unwrap();
    let token = event["token"].as_u64().unwrap();
    // 'a' is delivered before Alt+Tab changes focus during replay. Its release
    // and the Alt release must still complete the already-delivered chord.
    for command in [
        "key-down 30",
        "key-down 56",
        "key-down 15",
        "key-up 30",
        "key-up 15",
        "key-up 56",
    ] {
        action(&compositor, command);
    }
    action(&compositor, format!("shell-event-accept {token}"));
    let search = client.add(
        hearthspace_common::launcher_app_id_for_request(None, token),
        (500, 300),
        (0, 0),
        GREEN,
    );
    assert_eq!(
        query(&compositor, "status")["window_switcher"]["active"],
        false,
        "buffered Alt release must finish the switcher"
    );
    assert_eq!(
        client.keys().iter().filter(|key| **key == 30).count(),
        2,
        "the previously delivered text press needs its paired release"
    );
    client.destroy(search);
    click(&compositor, 10.0, 750.0);
    action(&compositor, format!("focus-window {id}"));
    assert_eq!(client.focus(), Some(normal));
    assert!(
        client.enter_keys().is_empty(),
        "no ghost held key may reach keyboard enter"
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
