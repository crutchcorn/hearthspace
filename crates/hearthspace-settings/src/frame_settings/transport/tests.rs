use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    thread,
};

fn serve(listener: UnixListener, replies: Vec<(&'static str, Vec<u8>)>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for (expected, response) in replies {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut command = String::new();
            BufReader::new(&mut stream).read_line(&mut command).unwrap();
            assert_eq!(command, expected);
            stream.write_all(&response).unwrap();
        }
    })
}

fn status(allow: bool) -> Vec<u8> {
    let json = format!("{{\"version\":1,\"allow_hide_tools\":{allow}}}");
    format!("ok {}\n{json}", json.len()).into_bytes()
}

#[test]
fn editor_restore_and_permission_commands_refresh_the_compositor_owned_preference() {
    for (operation, command, allow) in [
        (Operation::OpenEditor, "layout-tool\n", false),
        (
            Operation::RestoreTools,
            "frame-settings {\"restore_tools\":true}\n",
            true,
        ),
        (
            Operation::AllowHideTools(true),
            "frame-settings {\"allow_hide_tools\":true}\n",
            true,
        ),
        (
            Operation::AllowHideTools(false),
            "frame-settings {\"allow_hide_tools\":false}\n",
            false,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("command.sock");
        let server = serve(
            UnixListener::bind(&path).unwrap(),
            vec![
                (command, b"ok\n".to_vec()),
                ("frame-settings-status\n", status(allow)),
            ],
        );
        let response = execute(&path, operation);
        assert!(response.action_error.is_none());
        assert_eq!(response.snapshot.unwrap().allow_hide_tools, allow);
        server.join().unwrap();
    }
}

#[test]
fn rejected_restore_still_reports_the_actual_preference() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("command.sock");
    let server = serve(
        UnixListener::bind(&path).unwrap(),
        vec![
            (
                "frame-settings {\"restore_tools\":true}\n",
                b"err layout is locked\n".to_vec(),
            ),
            ("frame-settings-status\n", status(true)),
        ],
    );
    let response = execute(&path, Operation::RestoreTools);
    assert_eq!(response.action_error.as_deref(), Some("layout is locked"));
    assert!(response.snapshot.unwrap().allow_hide_tools);
    server.join().unwrap();
}

#[test]
fn malformed_status_never_silently_turns_off_the_preference() {
    for response in [
        b"ok 65537\n".to_vec(),
        b"ok 13\n{\"version\":2}".to_vec(),
        b"ok 13\n{\"version\":1}".to_vec(),
        b"ok 80\n{\"version\":1}".to_vec(),
        b"err session is locked\n".to_vec(),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("command.sock");
        let server = serve(
            UnixListener::bind(&path).unwrap(),
            vec![("frame-settings-status\n", response)],
        );
        assert!(execute(&path, Operation::Refresh).snapshot.is_err());
        server.join().unwrap();
    }
}
