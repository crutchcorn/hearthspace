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
            .map(|request| request.control.unwrap())
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
    assert_eq!(events.pending().last().unwrap().token, 2);
    assert!(events.pending().is_empty());
    drop(events);
    assert!(!path.exists());
}
