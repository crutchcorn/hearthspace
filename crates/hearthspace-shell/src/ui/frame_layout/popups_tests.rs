//! Exercise the asynchronous anchor boundary with the actual shell event handler.
use crate::ui::state::ShellState;

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
    let (key, popup) = deferred.open.unwrap();
    super::open(&mut state, &key, popup);
    assert!(state.outputs[1].show_search);
    assert_eq!(state.outputs[1].search_request_token, Some(2));
    assert_eq!(state.outputs[1].launcher_app_id(), anchor["app_id"]);
    assert!(!state.outputs[0].show_search);

    // Another action can open and edit a launcher while the anchor reply is in flight.
    state.dismiss_panels();
    apply_request(&mut state, 3);
    let (key, popup) = requests.try_recv().unwrap().open.unwrap();
    super::open(&mut state, &key, super::Popup::Apps);
    state.change_query(&key, "newer query".into());
    super::open(&mut state, &key, popup);
    assert!(state.outputs[1].show_search);
    assert_eq!(state.outputs[1].query, "newer query");
    assert_eq!(state.outputs[1].search_request_token, None);

    state.dismiss_panels();
    apply_request(&mut state, 4);
    let (key, popup) = requests.try_recv().unwrap().open.unwrap();
    state.frame_layout.snapshot.editing = true;
    super::open(&mut state, &key, popup);
    assert!(
        !state.outputs[1].show_search,
        "an editor opened during the anchor request still suppresses the popup"
    );
    assert_eq!(state.outputs[1].search_request_token, None);
    server.join().unwrap();
}
