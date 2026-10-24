//! Power completion owns its visible panel, never a subsequent app launch.
use super::*;
use crate::ui::{
    launch::{self, tests::worker},
    session_controls::Confirmation,
};
use std::{io::Write, os::unix::net::UnixListener, thread};

const OUTPUT: &str = "eDP-1";

fn pending_reboot(socket: PathBuf) -> (ShellState, tokio::sync::mpsc::Receiver<(String, Action)>) {
    let mut state = ShellState::new(socket, vec![OUTPUT.into()]);
    let mut commands = test_channel(&mut state);
    state.power.status.reboot_available = true;
    state.toggle_power(OUTPUT);
    state.outputs[0].power_confirmation = Some(Confirmation::Reboot);
    request_action(&mut state, OUTPUT, Action::Reboot);
    assert_eq!(
        commands.try_recv().unwrap(),
        (OUTPUT.to_owned(), Action::Reboot)
    );
    assert!(state.power.pending);
    assert_eq!(state.power.pending_action, Some(Action::Reboot));
    assert!(!state.power.can_action(Action::Reboot));
    (state, commands)
}

fn late_power_reply_preserves_new_launch(result: Result<(), String>) {
    let directory = tempfile::tempdir().unwrap();
    let (mut state, _commands) = pending_reboot(directory.path().join("control.sock"));
    // Close is available while reboot is pending. The next interaction belongs
    // to the launcher, including its independent cancellation guard and error.
    state.dismiss_power(OUTPUT);
    state.toggle_search(OUTPUT);
    state.change_query(OUTPUT, "editor".into());
    let mut launches = launch::tests::launch_test_queue(&mut state);
    launch::app(&mut state, OUTPUT, "editor.desktop", Some(OUTPUT), false);
    let request = launches.try_recv().unwrap();
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let socket = state.command_socket.clone();
    let launching = thread::spawn(move || {
        let result = request.perform(&socket);
        (request, result)
    });
    // Accept the real lifetime-bearing launch request but withhold its reply
    // until the earlier power operation has delivered its UI completion.
    let (mut peer, line) = worker::accept(&listener);
    let envelope = hearthspace_ipc::transport::launch::Envelope::decode(line.as_bytes())
        .unwrap()
        .expect("launch requests retain cancellation ownership");
    assert_eq!(
        envelope.command,
        crate::ShellCommand::LaunchAppOnOutput {
            app_id: "editor.desktop".into(),
            output_name: OUTPUT.into(),
        }
        .wire_name()
    );
    state.outputs[0].launch_error = Some("Current launcher feedback".into());

    apply(&mut state, Update::Finished(OUTPUT.into(), result));

    assert!(!state.power.pending);
    assert_eq!(state.power.pending_action, None);
    assert!(state.power.can_action(Action::Reboot));
    let output = &state.outputs[0];
    assert!(output.show_search);
    assert!(!output.show_power);
    assert_eq!(output.power_confirmation, None);
    assert_eq!(output.query, "editor");
    assert_eq!(
        output.launch_error.as_deref(),
        Some("Current launcher feedback")
    );
    assert!(output.launch_pending.is_some());

    // Retaining a flag alone is insufficient: dropping the real Pending would
    // shut down this socket and make Request::perform return a cancelled error.
    peer.write_all(b"ok\n").unwrap();
    let (request, result) = launching.join().unwrap();
    assert_eq!(result, Ok(()));
    launch::finished(&mut state, request, result, true);
    assert!(state.outputs[0].launch_pending.is_none());
    assert!(!state.outputs[0].show_search);
}

#[test]
fn late_power_success_preserves_the_new_launcher_and_its_live_request() {
    late_power_reply_preserves_new_launch(Ok(()));
}

#[test]
fn late_power_error_preserves_the_new_launcher_and_its_live_request() {
    late_power_reply_preserves_new_launch(Err("Earlier reboot was denied".into()));
}

#[test]
fn current_power_success_closes_only_power_and_releases_the_global_gate() {
    let (mut state, _commands) = pending_reboot("/unused".into());
    let mut launches = launch::tests::launch_test_queue(&mut state);
    launch::app(&mut state, OUTPUT, "editor.desktop", Some(OUTPUT), true);
    let _request = launches.try_recv().unwrap();
    state.outputs[0].launch_error = Some("Earlier power error".into());

    apply(&mut state, Update::Finished(OUTPUT.into(), Ok(())));

    assert!(!state.power.pending);
    assert_eq!(state.power.pending_action, None);
    assert!(state.power.can_action(Action::Reboot));
    assert!(!state.outputs[0].show_power);
    assert_eq!(state.outputs[0].power_confirmation, None);
    assert_eq!(state.outputs[0].launch_error, None);
    assert!(state.outputs[0].launch_pending.is_some());
}

#[test]
fn current_power_error_keeps_confirmation_and_feedback_available_for_retry() {
    let (mut state, mut commands) = pending_reboot("/unused".into());

    apply(
        &mut state,
        Update::Finished(OUTPUT.into(), Err("Reboot was denied".into())),
    );

    assert!(!state.power.pending);
    assert_eq!(state.power.pending_action, None);
    assert!(state.power.can_action(Action::Reboot));
    assert!(state.outputs[0].show_power);
    assert_eq!(
        state.outputs[0].power_confirmation,
        Some(Confirmation::Reboot)
    );
    assert_eq!(
        state.outputs[0].launch_error.as_deref(),
        Some("Reboot was denied")
    );
    request_action(&mut state, OUTPUT, Action::Reboot);
    assert_eq!(
        commands.try_recv().unwrap(),
        (OUTPUT.into(), Action::Reboot)
    );
    assert_eq!(state.outputs[0].launch_error, None);
    assert!(state.power.pending);
}

#[test]
fn removed_output_completion_releases_gate_without_changing_another_power_panel() {
    let (mut state, _commands) = pending_reboot("/unused".into());
    state.apply_output_names(vec!["DP-2".into()]);
    state.toggle_power("DP-2");
    state.outputs[0].launch_error = Some("Other output feedback".into());

    apply(
        &mut state,
        Update::Finished(OUTPUT.into(), Err("Old output power error".into())),
    );

    assert!(!state.power.pending);
    assert_eq!(state.power.pending_action, None);
    assert!(state.power.can_action(Action::Reboot));
    assert!(state.outputs[0].show_power);
    assert_eq!(
        state.outputs[0].launch_error.as_deref(),
        Some("Other output feedback")
    );
}

#[test]
fn queue_failure_reports_only_to_visible_power_ui() {
    let mut state = ShellState::new("/unused".into(), vec![OUTPUT.into()]);
    drop(test_channel(&mut state));
    state.power.status.reboot_available = true;
    state.toggle_power(OUTPUT);
    state.outputs[0].power_confirmation = Some(Confirmation::Reboot);

    request_action(&mut state, OUTPUT, Action::Reboot);

    assert!(!state.power.pending);
    assert_eq!(state.power.pending_action, None);
    assert!(state.outputs[0].show_power);
    assert_eq!(
        state.outputs[0].power_confirmation,
        Some(Confirmation::Reboot)
    );
    assert_eq!(
        state.outputs[0].launch_error.as_deref(),
        Some("Power controls are busy. Try again shortly.")
    );
    state.dismiss_power(OUTPUT);
    state.change_query(OUTPUT, "editor".into());
    state.outputs[0].launch_error = Some("New launcher error".into());

    request_action(&mut state, OUTPUT, Action::Reboot);

    assert!(!state.power.pending);
    assert!(state.outputs[0].show_search);
    assert_eq!(state.outputs[0].query, "editor");
    assert_eq!(
        state.outputs[0].launch_error.as_deref(),
        Some("New launcher error")
    );
}
