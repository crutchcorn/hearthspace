use super::*;

#[test]
fn a_rejected_change_preserves_the_confirmed_value_and_can_be_retried() {
    let (sender, mut requests) = tokio::sync::mpsc::unbounded_channel();
    let mut state = State {
        sender: Some(sender),
        ..Default::default()
    };
    state.receive(Response {
        operation: Operation::Refresh,
        snapshot: Ok(Snapshot {
            allow_hide_tools: false,
        }),
        action_error: None,
    });
    state.request(Operation::AllowHideTools(true));
    assert_eq!(
        requests.try_recv().unwrap(),
        Operation::AllowHideTools(true)
    );
    assert!(
        !state.snapshot.unwrap().allow_hide_tools,
        "wait for the compositor acknowledgment"
    );
    state.receive(Response {
        operation: Operation::AllowHideTools(true),
        snapshot: Ok(Snapshot {
            allow_hide_tools: false,
        }),
        action_error: Some("Could not save the layout".into()),
    });
    assert!(state.enabled());
    assert_eq!(state.message(), Some("Could not save the layout"));
    assert!(!state.snapshot.unwrap().allow_hide_tools);
    state.request(Operation::AllowHideTools(true));
    assert_eq!(
        requests.try_recv().unwrap(),
        Operation::AllowHideTools(true)
    );
    state.receive(Response {
        operation: Operation::AllowHideTools(true),
        snapshot: Ok(Snapshot {
            allow_hide_tools: true,
        }),
        action_error: None,
    });
    assert!(state.snapshot.unwrap().allow_hide_tools);
    assert_eq!(state.message(), None);
}

#[test]
fn disconnect_retains_the_value_and_reconnect_updates_external_changes() {
    let (sender, mut requests) = tokio::sync::mpsc::unbounded_channel();
    let mut state = State {
        sender: Some(sender),
        ..Default::default()
    };
    state.receive(Response {
        operation: Operation::Refresh,
        snapshot: Ok(Snapshot {
            allow_hide_tools: true,
        }),
        action_error: None,
    });
    state.receive(Response {
        operation: Operation::Refresh,
        snapshot: Err("disconnected".into()),
        action_error: None,
    });
    assert!(!state.enabled());
    assert!(state.snapshot.unwrap().allow_hide_tools);
    state.request(Operation::RestoreTools);
    assert!(requests.try_recv().is_err());
    state.receive(Response {
        operation: Operation::Refresh,
        snapshot: Ok(Snapshot {
            allow_hide_tools: false,
        }),
        action_error: None,
    });
    assert!(state.enabled());
    assert!(!state.snapshot.unwrap().allow_hide_tools);
    assert_eq!(state.message(), None);
}
