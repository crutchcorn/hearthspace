//! Exercise the asynchronous anchor boundary with the actual shell event handler.
use crate::ui::state::ShellState;

pub(in crate::ui) fn cancellation_rejects_queued_open(cancel: impl FnOnce(&mut ShellState)) {
    use std::time::Instant;
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    let snapshot = super::super::Snapshot {
        version: 1,
        outputs: vec![super::super::Output {
            output: "eDP-1".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    super::super::apply(&mut state, snapshot.clone());
    let (send, mut receive) = tokio::sync::mpsc::channel(4);
    state.frame_layout.requests = Some(send);
    state.toggle_notifications("eDP-1");
    let now = Instant::now();
    let opening = receive
        .try_recv()
        .unwrap()
        .open
        .unwrap()
        .acknowledge(1, now);
    cancel(&mut state);
    // Even if lock/unlock fell entirely between layout polls, cancellation wins.
    let mut snapshot = snapshot;
    snapshot.popup_epoch = state.frame_layout.popup_epoch;
    super::super::apply(&mut state, snapshot);
    opening.deliver(&mut state, now);
    assert!(!state.outputs[0].show_notifications);
    state.toggle_notifications("eDP-1");
    receive
        .try_recv()
        .unwrap()
        .open
        .unwrap()
        .acknowledge(2, now)
        .deliver(&mut state, now);
    assert!(state.outputs[0].show_notifications);
}

pub(in crate::ui) fn deferred_keyboard_search(mut apply_request: impl FnMut(&mut ShellState, u64)) {
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
    let (sender, mut requests) = tokio::sync::mpsc::channel(4);
    state.frame_layout.visible = true;
    state.frame_layout.requests = Some(sender);

    apply_request(&mut state, 1);
    assert!(
        requests.try_recv().is_err(),
        "rejected keyboard requests must not enqueue a popup"
    );
    apply_request(&mut state, 2);
    assert!(
        !state.outputs[1].show_search,
        "anchoring must complete before a native window opens"
    );
    let deferred = requests.try_recv().unwrap();
    let anchor: serde_json::Value = serde_json::from_str(
        deferred
            .command
            .strip_prefix("frame-popup-anchor ")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(anchor["output"], "HDMI-A-1");
    assert_eq!(anchor["target"]["id"], "apps");
    assert_eq!(
        anchor["app_id"],
        hearthspace_common::launcher_app_id_for_request(Some("HDMI-A-1"), 2)
    );
    let intent = deferred.open.unwrap();
    super::open(&mut state, &intent.key, intent.popup);
    assert!(state.outputs[1].show_search);
    assert_eq!(state.outputs[1].search_request_token, Some(2));
    assert_eq!(state.outputs[1].launcher_app_id(), anchor["app_id"]);
    assert!(!state.outputs[0].show_search);

    // Another action can open and edit a launcher while the anchor reply is in flight.
    state.dismiss_panels();
    apply_request(&mut state, 3);
    let intent = requests.try_recv().unwrap().open.unwrap();
    super::open(&mut state, &intent.key, super::Popup::Apps);
    state.change_query(&intent.key, "newer query".into());
    super::open(&mut state, &intent.key, intent.popup);
    assert!(state.outputs[1].show_search);
    assert_eq!(state.outputs[1].query, "newer query");
    assert_eq!(state.outputs[1].search_request_token, None);

    state.dismiss_panels();
    apply_request(&mut state, 4);
    let intent = requests.try_recv().unwrap().open.unwrap();
    state.frame_layout.snapshot.editing = true;
    super::open(&mut state, &intent.key, intent.popup);
    assert!(
        !state.outputs[1].show_search,
        "an editor opened during the anchor request still suppresses the popup"
    );
    assert_eq!(state.outputs[1].search_request_token, None);
    server.join().unwrap();
}

pub(in crate::ui) fn deferred_toggle_requests(
    mut apply_request: impl FnMut(&mut ShellState, u64, bool),
) {
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
        thread,
        time::{Duration, Instant},
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        for token in 1..=7 {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline);
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut command = String::new();
            BufReader::new(&mut stream).read_line(&mut command).unwrap();
            assert_eq!(command, format!("shell-event-accept {token}\n"));
            stream.write_all(b"ok\n").unwrap();
        }
    });
    let mut state = ShellState::new(path, vec!["eDP-1".into()]);
    super::super::apply(
        &mut state,
        super::super::Snapshot {
            version: 1,
            outputs: vec![super::super::Output {
                output: "eDP-1".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    let (send, mut receive) = tokio::sync::mpsc::channel(4);
    state.frame_layout.requests = Some(send);
    let now = Instant::now();
    apply_request(&mut state, 1, true);
    let old = receive
        .try_recv()
        .unwrap()
        .open
        .unwrap()
        .acknowledge(1, now);
    apply_request(&mut state, 2, true);
    old.deliver(&mut state, now);
    assert!(!state.outputs[0].show_search);
    assert!(receive.try_recv().is_err());

    super::open(&mut state, "eDP-1", super::Popup::Apps);
    state.change_query("eDP-1", "old query".into());
    state.outputs[0].search_request_token = Some(99);
    apply_request(&mut state, 3, true);
    assert!(!state.outputs[0].show_search);
    assert!(receive.try_recv().is_err());
    apply_request(&mut state, 4, true);
    let request = receive.try_recv().unwrap();
    let anchor: serde_json::Value =
        serde_json::from_str(request.command.strip_prefix("frame-popup-anchor ").unwrap()).unwrap();
    assert_eq!(
        anchor["app_id"],
        hearthspace_common::launcher_app_id_for_request(Some("eDP-1"), 4)
    );
    request
        .open
        .unwrap()
        .acknowledge(4, now)
        .deliver(&mut state, now);
    assert!(state.outputs[0].show_search);
    assert!(state.outputs[0].query.is_empty());
    assert_eq!(state.outputs[0].search_request_token, Some(4));

    // A direct Apps reopening uses its own provenance, not the old request token.
    apply_request(&mut state, 5, true);
    state.toggle_search("eDP-1");
    receive
        .try_recv()
        .unwrap()
        .open
        .unwrap()
        .acknowledge(5, now)
        .deliver(&mut state, now);
    assert!(state.outputs[0].show_search);
    assert_eq!(state.outputs[0].search_request_token, None);
    state.dismiss_search("eDP-1");

    apply_request(&mut state, 6, false);
    let old = receive
        .try_recv()
        .unwrap()
        .open
        .unwrap()
        .acknowledge(6, now);
    apply_request(&mut state, 7, false);
    let newest = receive
        .try_recv()
        .unwrap()
        .open
        .unwrap()
        .acknowledge(7, now);
    old.deliver(&mut state, now);
    newest.deliver(&mut state, now);
    assert!(state.outputs[0].show_search);
    assert_eq!(state.outputs[0].search_request_token, Some(7));
    server.join().unwrap();
}
