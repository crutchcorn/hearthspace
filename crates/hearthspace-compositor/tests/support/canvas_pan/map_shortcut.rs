//! The map shortcut crosses the existing one-use, output-qualified shell bridge.
use super::*;
use std::{io::ErrorKind, os::unix::net::UnixDatagram};

#[test]
fn meta_overview_sends_one_action_per_press_to_the_pointer_output_without_key_leaks() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    let output = query(&compositor, "status")["outputs"][0]["name"].clone();
    let events = UnixDatagram::bind(hearthspace_common::shell_event_socket_path(
        &compositor.command_socket_path,
    ))
    .unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    motion(&compositor, (640., 400.));
    let before = viewport(&compositor);
    for _ in 0..2 {
        action(&compositor, "key-down 125");
        action(&compositor, "key-down 24");
        action(&compositor, "key-down 24");
        action(&compositor, "key-up 125");
        action(&compositor, "key-up 24");
        let event = crate::shell_events::receive(&events, "canvas-search-action");
        let command = format!("canvas-search-event-accept {}", event["token"]);
        let accepted = query(&compositor, &command);
        assert_eq!(accepted["id"], "tool:map");
        assert_eq!(accepted["output"], output);
        assert!(compositor.send_text_command(&command).starts_with("err "));
        assert_eq!(viewport(&compositor), before);
        assert!(
            client.take_keyboard_events().iter().all(|event| !matches!(
                event,
                wayland_client::protocol::wl_keyboard::Event::Key { key: 24, .. }
            )),
            "Overview key reached the application"
        );
    }
    events.set_nonblocking(true).unwrap();
    assert_eq!(
        crate::shell_events::next_action(&events)
            .unwrap_err()
            .kind(),
        ErrorKind::WouldBlock,
        "a repeat or Meta release emitted an extra shell action"
    );
}
