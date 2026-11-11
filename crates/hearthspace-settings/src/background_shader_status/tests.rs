use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
};

#[test]
fn reload_and_path_changes_reject_old_completions_including_after_reset() {
    let mut status = ShaderStatus::default();
    status.synchronize_path(Some("first.glsl"));
    let first = status.request.clone();
    status.reload();
    let reload = status.request.clone();
    status.receive(Response {
        request: first,
        result: Err("old compile error".into()),
    });
    assert_eq!(status.message, "Applying GLSL wallpaper…");
    assert!(!status.failed);
    status.receive(Response {
        request: reload.clone(),
        result: Err("bad GLSL".into()),
    });
    assert!(status.failed);
    assert_eq!(status.message, "bad GLSL");
    status.synchronize_path(Some("second.glsl"));
    let second = status.request.clone();
    status.receive(Response {
        request: reload,
        result: Ok("stale success".into()),
    });
    assert_eq!(status.message, "Applying GLSL wallpaper…");
    status.receive(Response {
        request: second.clone(),
        result: Ok("active".into()),
    });
    assert_eq!(status.message, "active");
    status.synchronize_path(None);
    status.receive(Response {
        request: second,
        result: Err("late error".into()),
    });
    assert!(!status.failed);
    assert!(status.request.path.is_none());
}

#[test]
fn reload_precedes_bounded_status_and_compile_failure_does_not_change_selection() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("compositor.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        for (expected, reply) in [
            ("reload-settings", "ok\n"),
            (
                "status",
                r#"{"background_shader":{"path":"chosen.glsl","ready":true,"error":"syntax error"}}"#,
            ),
            (
                "status",
                r#"{"background_shader":{"path":"chosen.glsl","ready":true,"error":null}}"#,
            ),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut command = String::new();
            BufReader::new(&stream).read_line(&mut command).unwrap();
            assert_eq!(command.trim(), expected);
            if expected == "status" {
                write!(stream, "ok {}\n", reply.len()).unwrap();
            }
            stream.write_all(reply.as_bytes()).unwrap();
        }
    });
    let request = Request {
        path: Some("chosen.glsl".into()),
        generation: 1,
    };
    let error = observe_at(&socket, &request, true).unwrap_err();
    assert!(error.contains("syntax error"));
    assert!(error.contains("Canvas dots"));
    assert_eq!(request.path.as_deref(), Some("chosen.glsl"));
    assert_eq!(
        observe_at(&socket, &request, false).unwrap(),
        "GLSL wallpaper is active on the desktop."
    );
    server.join().unwrap();
}

#[test]
fn unavailable_compositor_is_an_error_without_clearing_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        path: Some("chosen.glsl".into()),
        generation: 1,
    };
    assert!(observe_at(&dir.path().join("absent.sock"), &request, true).is_err());
    assert_eq!(request.path.as_deref(), Some("chosen.glsl"));
}
