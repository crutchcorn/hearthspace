use super::*;
#[path = "test_worker.rs"]
pub(in crate::ui) mod worker;

pub(in crate::ui) fn launch_test_queue(
    state: &mut ShellState,
) -> tokio::sync::mpsc::Receiver<Request> {
    let (sender, receiver) = tokio::sync::mpsc::channel(4);
    state.launch_requests = Some(sender);
    receiver
}
use std::{io::Write, os::unix::net::UnixListener, time::Duration};

pub(in crate::ui) fn finish_next(
    state: &mut ShellState,
    queue: &mut tokio::sync::mpsc::Receiver<Request>,
) {
    let request = queue.try_recv().expect("queued launch");
    let result = request.perform(&state.command_socket);
    finished(state, request, result, true);
}

#[test]
fn launch_callback_returns_before_acceptance_and_preserves_query_on_failure() {
    let root = tempfile::tempdir().unwrap();
    let mut state = ShellState::new(root.path().join("control.sock"), vec!["eDP-1".into()]);
    state.outputs[0].show_search = true;
    state.outputs[0].query = "editor".into();
    let mut queue = launch_test_queue(&mut state);
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let (ready, received) = std::sync::mpsc::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = worker::read_request(&mut stream);
        assert_eq!(
            request,
            format!(
                "{}\n",
                crate::ShellCommand::LaunchAppOnOutput {
                    app_id: "editor.desktop".into(),
                    output_name: "HDMI-A-1".into()
                }
                .wire_name()
            )
        );
        ready.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(2)).unwrap();
        stream
            .write_all(b"err fixture activation rejected\n")
            .unwrap();
    });
    app(
        &mut state,
        "eDP-1",
        "editor.desktop",
        Some("HDMI-A-1"),
        false,
    );
    app(
        &mut state,
        "eDP-1",
        "editor.desktop",
        Some("HDMI-A-1"),
        false,
    );
    let request = queue.try_recv().unwrap();
    assert!(
        queue.try_recv().is_err(),
        "repeated pending clicks must not duplicate launch"
    );
    assert!(state.outputs[0].launch_pending.is_some());
    let socket = state.command_socket.clone();
    let worker = std::thread::spawn(move || {
        let result = request.perform(&socket);
        (request, result)
    });
    received.recv_timeout(Duration::from_secs(1)).unwrap();
    // Unrelated UI state can change while the real IPC reply is held.
    state.clock = "still responsive".into();
    assert_eq!(state.outputs[0].query, "editor");
    release.send(()).unwrap();
    let (request, result) = worker.join().unwrap();
    finished(&mut state, request, result, true);
    assert!(state.outputs[0].show_search);
    assert_eq!(state.outputs[0].query, "editor");
    assert!(
        state.outputs[0]
            .launch_error
            .as_deref()
            .unwrap()
            .contains("activation rejected")
    );
    assert!(state.outputs[0].launch_pending.is_none());
    server.join().unwrap();
}

#[test]
fn dismissed_or_replaced_outputs_reject_queued_work_and_ignore_late_replies() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    let mut queue = launch_test_queue(&mut state);
    state.outputs[0].show_search = true;
    app(&mut state, "eDP-1", "editor.desktop", None, false);
    let request = queue.try_recv().unwrap();
    state.dismiss_search("eDP-1");
    assert!(
        request
            .perform(Path::new("/unused"))
            .unwrap_err()
            .contains("cancelled")
    );
    state.apply_output_names(vec!["HDMI-A-1".into()]);
    state.apply_output_names(vec!["eDP-1".into()]);
    state.outputs[0].query = "replacement".into();
    finished(&mut state, request, Err("stale failure".into()), true);
    assert_eq!(state.outputs[0].query, "replacement");
    assert!(state.outputs[0].launch_error.is_none());
    assert!(!state.outputs[0].show_search);
}

#[test]
fn gesture_generation_changes_and_inactive_sessions_suppress_late_feedback() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    let mut queue = launch_test_queue(&mut state);
    state.outputs[0]
        .drawing
        .start(crate::ui::favorite_state::Mode::Launch);
    let generation = state.outputs[0].drawing.generation;
    submit(
        &mut state,
        "eDP-1",
        "unused".into(),
        Some(generation),
        false,
    );
    let request = queue.try_recv().unwrap();
    state.outputs[0].drawing.refresh_generation();
    finished(&mut state, request, Err("stale failure".into()), true);
    assert!(state.outputs[0].launch_error.is_none());
    app(&mut state, "eDP-1", "editor.desktop", None, true);
    let request = queue.try_recv().unwrap();
    finished(&mut state, request, Err("inactive failure".into()), false);
    assert!(!state.outputs[0].show_search);
    assert!(state.outputs[0].launch_error.is_none());
}

#[test]
fn launch_queue_is_bounded_and_expired_requests_never_contact_the_endpoint() {
    let outputs = (0..5).map(|index| format!("output-{index}")).collect();
    let mut state = ShellState::new("/unused".into(), outputs);
    let mut queue = launch_test_queue(&mut state);
    for index in 0..5 {
        app(
            &mut state,
            &format!("output-{index}"),
            "editor.desktop",
            None,
            true,
        );
    }
    assert!(state.outputs[4].launch_pending.is_none());
    assert!(state.outputs[4].show_search);
    assert!(
        state.outputs[4]
            .launch_error
            .as_deref()
            .unwrap()
            .contains("busy")
    );
    let mut request = queue.try_recv().unwrap();
    request.deadline = std::time::Instant::now() - Duration::from_millis(1);
    assert!(
        request
            .perform(Path::new("/unused"))
            .unwrap_err()
            .contains("expired")
    );
    assert_eq!(queue.len(), 3);
}

#[test]
fn dismissing_after_transport_started_closes_the_pending_preparation_request() {
    use std::io::Read;
    let root = tempfile::tempdir().unwrap();
    let mut state = ShellState::new(root.path().join("control.sock"), vec!["eDP-1".into()]);
    let mut queue = launch_test_queue(&mut state);
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let (entered, ready) = std::sync::mpsc::channel();
    let (closed, revoked) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        assert_eq!(
            worker::read_request(&mut peer),
            "launch-app editor.desktop\n"
        );
        entered.send(()).unwrap();
        let mut byte = [0];
        assert_eq!(peer.read(&mut byte).unwrap(), 0);
        closed.send(()).unwrap();
    });
    state.outputs[0].show_search = true;
    app(&mut state, "eDP-1", "editor.desktop", None, false);
    let request = queue.try_recv().unwrap();
    let socket = state.command_socket.clone();
    let work = std::thread::spawn(move || {
        let result = request.perform(&socket);
        (request, result)
    });
    ready.recv_timeout(Duration::from_secs(1)).unwrap();
    state.dismiss_search("eDP-1");
    revoked
        .recv_timeout(Duration::from_secs(1))
        .expect("dismissal must revoke preparation before acceptance");
    let (request, result) = work.join().unwrap();
    assert!(result.as_ref().unwrap_err().contains("cancelled"));
    finished(&mut state, request, result, true);
    assert!(!state.outputs[0].show_search);
    assert!(state.outputs[0].launch_error.is_none());
    server.join().unwrap();
}
