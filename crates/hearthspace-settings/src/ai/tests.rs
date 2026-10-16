use super::*;
use xilem::tokio::sync::mpsc::unbounded_channel;

pub(crate) fn snapshot(installed: bool, downloaded: bool) -> ModelStatus {
    ModelStatus {
        installed,
        downloaded,
        warm: false,
        device: DevicePreference::Automatic,
        runtime_error: None,
    }
}

#[test]
fn duplicate_actions_are_not_queued_during_model_operations() {
    let (sender, mut requests) = unbounded_channel();
    let mut state = AiSettings {
        sender: Some(sender),
        ..Default::default()
    };
    state.request(Operation::Install);
    state.request(Operation::Remove);
    state.request(Operation::Refresh);
    assert_eq!(requests.try_recv().unwrap(), Operation::Install);
    assert!(requests.try_recv().is_err());
    assert_eq!(state.pending, Some(Operation::Install));
}

#[test]
fn partial_download_failure_keeps_cleanup_available_and_clears_busy_state() {
    let mut state = AiSettings {
        pending: Some(Operation::Install),
        ..Default::default()
    };
    state.receive(Response::Finished {
        operation: Operation::Install,
        snapshot: Some(snapshot(false, true)),
        error: Some("Ollama is unavailable".into()),
    });
    assert!(state.pending.is_none());
    assert!(!state.installed());
    assert!(state.can_remove());
    assert!(state.message.contains("Ollama is unavailable"));
}

#[test]
fn progress_is_indeterminate_for_import_and_cannot_leak_into_other_actions() {
    let mut state = AiSettings {
        pending: Some(Operation::Install),
        ..Default::default()
    };
    state.receive(Response::Progress(InstallProgress {
        phase: InstallPhase::Downloading,
        completed: 1_000_000,
        total: Some(2_000_000),
    }));
    assert_eq!(state.progress, Some(0.5));
    assert!(state.message.contains("1 / 2 MB"));
    state.receive(Response::Progress(InstallProgress {
        phase: InstallPhase::Importing,
        completed: 0,
        total: None,
    }));
    assert!(state.progress.is_none());
    assert!(state.message.contains("Registering"));
    state.pending = Some(Operation::Remove);
    let message = state.message.clone();
    state.receive(Response::Progress(InstallProgress {
        phase: InstallPhase::Warming,
        completed: 0,
        total: None,
    }));
    assert_eq!(state.message, message);
}

#[test]
fn unreachable_runtime_is_never_reported_as_ready() {
    let mut status = snapshot(true, true);
    status.runtime_error = Some("Ollama decision support is unavailable".into());
    let mut state = AiSettings::default();
    state.receive(Response::Finished {
        operation: Operation::Refresh,
        snapshot: Some(status),
        error: None,
    });
    assert_eq!(state.message, "Ollama decision support is unavailable");
    assert!(state.can_remove());
}

#[test]
fn worker_failure_does_not_leave_controls_permanently_busy() {
    let (sender, receiver) = unbounded_channel();
    drop(receiver);
    let mut state = AiSettings {
        sender: Some(sender),
        ..Default::default()
    };
    state.request(Operation::Refresh);
    assert!(state.pending.is_none());
    assert!(state.message.contains("Reopen Settings"));
}
