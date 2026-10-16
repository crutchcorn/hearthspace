use super::*;

#[test]
fn device_events_preserve_key_order_and_reject_unknown_actions_or_search_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let command = dir.path().join("control.sock");
    let events = ShellEvents::bind(&command).unwrap();
    let sender = UnixDatagram::unbound().unwrap();
    for (index, control) in ["volume-up", "brightness-down", "volume-mute"]
        .into_iter()
        .enumerate()
    {
        let value = serde_json::json!({"version":1,"action":"device-control","control":control,
            "token":(1u64 << 63) + index as u64 + 1,"output":"eDP-1"});
        sender
            .send_to(
                &serde_json::to_vec(&value).unwrap(),
                shell_event_socket_path(&command),
            )
            .unwrap();
    }
    let pending = events.pending();
    assert_eq!(
        pending
            .iter()
            .map(|event| match event {
                Event::Request(request) => request.control.unwrap(),
                _ => panic!("expected device request"),
            })
            .collect::<Vec<_>>(),
        [
            super::super::control_state::Action::VolumeStep(5),
            super::super::control_state::Action::BrightnessStep(-5),
            super::super::control_state::Action::ToggleMute,
        ]
    );
    for control in ["volume-up", "run-command"] {
        let low = serde_json::json!({"version":1,"action":"device-control","control":control,"token":4,"output":null});
        assert!(parse_request(&serde_json::to_vec(&low).unwrap()).is_none());
    }
    let unknown = serde_json::json!({"version":1,"action":"device-control","control":"run-command","token":1u64 << 63,"output":null});
    assert!(parse_request(&serde_json::to_vec(&unknown).unwrap()).is_none());
}

#[test]
fn events_validate_version_action_token_output_and_size() {
    let valid = br#"{"version":1,"action":"toggle-search","token":2,"output":"eDP-1"}"#;
    assert_eq!(
        parse_request(valid),
        Some(ShellRequest {
            token: 2,
            toggle: true,
            control: None,
            output: Some("eDP-1".into())
        })
    );
    for invalid in [
        br#"{"version":2,"action":"toggle-search","token":2,"output":null}"#.as_slice(),
        br#"{"version":1,"action":"quit","token":2,"output":null}"#,
        br#"{"version":1,"action":"open-search","token":0,"output":null}"#,
        br#"{"version":1,"action":"open-search","token":2,"output":12}"#,
        br#"{"version":1,"action":"open-search","token":2}"#,
    ] {
        assert!(parse_request(invalid).is_none());
    }
    assert!(parse_request(&vec![b' '; 4097]).is_none());
}

#[test]
fn search_requests_require_current_token_and_open_is_idempotent() {
    use std::{
        io::{Read, Write},
        os::unix::net::UnixListener,
        thread,
    };
    let dir = tempfile::tempdir().unwrap();
    let command = dir.path().join("control.sock");
    let listener = UnixListener::bind(&command).unwrap();
    let server = thread::spawn(move || {
        for token in 1..=4 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            stream.read_to_string(&mut request).unwrap();
            assert_eq!(request, format!("shell-event-accept {token}\n"));
            stream
                .write_all(if token == 1 {
                    b"err stale token\n"
                } else {
                    b"ok\n"
                })
                .unwrap();
        }
    });
    let mut state = ShellState::new(command, vec!["eDP-1".into(), "HDMI-A-1".into()]);
    let mut request = ShellRequest {
        token: 1,
        toggle: false,
        control: None,
        output: Some("HDMI-A-1".into()),
    };
    apply_request(&mut state, request.clone());
    assert!(!state.outputs[1].show_search);
    request.token = 2;
    apply_request(&mut state, request.clone());
    assert!(state.outputs[1].show_search);
    assert_eq!(state.outputs[1].search_request_token, Some(2));
    assert!(state.outputs[1].launcher_app_id().ends_with(".request-2"));
    state.change_query("HDMI-A-1", "keep this query".into());
    request.token = 3;
    apply_request(&mut state, request.clone());
    assert_eq!(state.outputs[1].query, "keep this query");
    assert_eq!(state.outputs[1].search_request_token, Some(2));
    request.token = 4;
    request.toggle = true;
    apply_request(&mut state, request);
    assert!(!state.outputs[1].show_search);
    assert_eq!(state.outputs[1].search_request_token, None);
    state.toggle_search("HDMI-A-1");
    assert_eq!(state.outputs[1].search_request_token, None);
    assert!(!state.outputs[1].launcher_app_id().contains(".request-"));
    assert!(!state.outputs[0].show_search);
    server.join().unwrap();
}

#[test]
fn listener_rejects_duplicates_and_only_removes_its_own_socket() {
    let dir = tempfile::tempdir().unwrap();
    let command = dir.path().join("control.sock");
    let events = ShellEvents::bind(&command).unwrap();
    let path = shell_event_socket_path(&command);
    assert_eq!(path.metadata().unwrap().mode() & 0o777, 0o600);
    assert!(ShellEvents::bind(&command).is_err());
    std::fs::remove_file(&path).unwrap();
    let replacement = UnixDatagram::bind(&path).unwrap();
    drop(events);
    assert!(path.exists());
    drop(replacement);
}

#[test]
fn listener_recovers_stale_socket_but_preserves_regular_files_and_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let command = dir.path().join("control.sock");
    let path = shell_event_socket_path(&command);
    drop(UnixDatagram::bind(&path).unwrap());
    let events = ShellEvents::bind(&command).unwrap();
    drop(events);
    std::fs::write(&path, b"keep me").unwrap();
    assert!(ShellEvents::bind(&command).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
    std::fs::remove_file(&path).unwrap();
    let other = dir.path().join("other.sock");
    drop(UnixDatagram::bind(&other).unwrap());
    std::os::unix::fs::symlink(&other, &path).unwrap();
    assert!(ShellEvents::bind(&command).is_err());
    assert!(path.symlink_metadata().unwrap().file_type().is_symlink());
    assert!(other.exists());
}

#[test]
fn listener_drains_to_latest_valid_request_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let command = dir.path().join("control.sock");
    let events = ShellEvents::bind(&command).unwrap();
    let path = shell_event_socket_path(&command);
    let sender = UnixDatagram::unbound().unwrap();
    for token in [1, 2] {
        let bytes =
            format!(r#"{{"version":1,"action":"open-search","token":{token},"output":null}}"#);
        sender.send_to(bytes.as_bytes(), &path).unwrap();
    }
    sender.send_to(b"bad json", &path).unwrap();
    assert!(matches!(events.pending().last(), Some(Event::Request(request)) if request.token == 2));
    assert!(events.pending().is_empty());
    drop(events);
    assert!(!path.exists());
}

#[test]
fn search_coalescing_never_drops_or_reorders_gesture_packets() {
    let dir = tempfile::tempdir().unwrap();
    let command = dir.path().join("control.sock");
    let events = ShellEvents::bind(&command).unwrap();
    let path = shell_event_socket_path(&command);
    let sender = UnixDatagram::unbound().unwrap();
    let search =
        |token| serde_json::json!({"version":1,"action":"open-search","token":token,"output":null});
    let gesture = |sequence, phase| {
        serde_json::json!({"version":1,"action":"favorite-gesture",
        "generation":42,"stroke":7,"sequence":sequence,"phase":phase,"x":20,"y":20,"output":null})
    };
    for value in [
        search(1),
        gesture(0, "begin"),
        gesture(1, "update"),
        search(2),
        gesture(2, "end"),
    ] {
        sender
            .send_to(&serde_json::to_vec(&value).unwrap(), &path)
            .unwrap();
    }
    let pending = events.pending();
    assert_eq!(pending.len(), 4);
    assert!(matches!(&pending[0], Event::Gesture(event) if event.sequence == 0));
    assert!(matches!(&pending[1], Event::Gesture(event) if event.sequence == 1));
    assert!(matches!(&pending[2], Event::Request(request) if request.token == 2));
    assert!(matches!(&pending[3], Event::Gesture(event) if event.sequence == 2));
    assert!(parse_event(br#"{"version":2,"action":"favorite-gesture"}"#).is_none());
    assert!(parse_event(&vec![b' '; 4097]).is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn async_readiness_keeps_a_bounded_batches_tail_and_rearms_after_would_block() {
    // A connected pair can queue more than the unconnected filesystem socket's
    // small datagram limit. Preload >32 packets to exercise the batch boundary.
    let dir = tempfile::tempdir().unwrap();
    let (sender, socket) = UnixDatagram::pair().unwrap();
    sender.set_nonblocking(true).unwrap();
    socket.set_nonblocking(true).unwrap();
    let events = ShellEvents {
        socket,
        path: dir.path().join("not-bound.sock"),
        identity: (0, 0),
    };
    let payload = |sequence| {
        serde_json::to_vec(&serde_json::json!({"version":1,
        "action":"favorite-gesture","generation":42,"stroke":7,"sequence":sequence,
        "phase":"update","x":20,"y":20,"output":null}))
        .unwrap()
    };
    for sequence in 0..64 {
        sender.send(&payload(sequence)).unwrap();
    }
    // SAFETY: events owns this stable descriptor until after the local
    // registration is dropped; the test never moves or forgets either value.
    let socket =
        unsafe { AsyncFd::register_with_interest(events.socket.as_fd(), Interest::READABLE) }
            .unwrap();
    for batch in 0..2 {
        let pending = tokio::time::timeout(Duration::from_secs(2), receive(&events, &socket))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pending.len(), 32);
        for (index, event) in pending.iter().enumerate() {
            assert!(
                matches!(event, Event::Gesture(event) if event.sequence == batch * 32 + index as u64)
            );
        }
    }
    assert!(receive(&events, &socket).await.unwrap().is_empty());
    sender.send(&payload(64)).unwrap();
    let pending = tokio::time::timeout(Duration::from_secs(2), receive(&events, &socket))
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(pending.as_slice(), [Event::Gesture(event)] if event.sequence == 64));
}
