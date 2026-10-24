use super::*;
use crate::settings_refresh::{apply_refresh, run_refresh};
use hearthspace_ipc::settings_snapshot::snapshot_response;

fn inverse_order_workers(initial: SettingsSnapshot) {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("settings.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let (_state_directory, mut state, receiver) = state();
    let (refresh_messages, refresh_received) = mpsc::channel();
    let generation = state.preferences.generation.clone();
    let refresh_socket = socket.clone();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let refresh_worker = thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            tokio::select! {
                _ = stopped => {},
                _ = run_refresh(generation, Ok(refresh_socket), |message| refresh_messages.send(message).is_ok()) => {},
            }
        });
    });
    // Hold the initial GET until a layout request is also outstanding.
    let mut initial_read = accept(&listener);
    assert_eq!(read_line(&initial_read), "get snapshot\n");
    save_monitor_scale(&mut state, "primary", 2);
    let (responses, received) = mpsc::channel();
    let save_worker = thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run_worker(
                receiver,
                Endpoints {
                    settings: Ok(socket),
                },
                move |response| responses.send(response).is_ok(),
            ));
    });
    let mut commit = accept(&listener);
    let layout = read_layout(&commit);
    assert_eq!(layout[0].scale, 2);
    initial_read
        .write_all(snapshot_response(Some(initial.clone())).unwrap().as_bytes())
        .unwrap();
    let observation = refresh_received
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    assert_eq!(observation.0, 0);
    apply_refresh(&mut state, observation);
    assert_eq!(state.settings.monitors[0].scale, 2);
    assert_eq!(state.saved_monitors[0].scale, 1);
    let mut outstanding_watch = accept(&listener);
    assert!(read_line(&outstanding_watch).starts_with("watch settings "));

    // The observed GET (or appearance commit) actually preceded this write.
    let committed = SettingsSnapshot::new(
        10,
        Settings {
            monitors: layout,
            ..initial.settings
        },
    )
    .unwrap();
    commit
        .write_all(
            snapshot_response(Some(committed.clone()))
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
    receive_display_layout(
        &mut state,
        received.recv_timeout(Duration::from_secs(2)).unwrap(),
    );
    assert_eq!(
        state.settings.monitors[0].scale, 2,
        "an earlier observation must not roll back the acknowledged edit"
    );
    assert!(
        state.display_layout.is_pending(),
        "ordering is unresolved until a post-commit read"
    );
    outstanding_watch
        .write_all(snapshot_response(None).unwrap().as_bytes())
        .unwrap();
    let mut after_commit = accept(&listener);
    assert_eq!(
        read_line(&after_commit),
        "get snapshot\n",
        "acknowledgement must force a new GET rather than reuse the earlier cursor"
    );
    assert_eq!(state.settings.monitors[0].scale, 2);
    after_commit
        .write_all(
            snapshot_response(Some(committed.clone()))
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
    let observation = refresh_received
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    assert_eq!(observation.0, 1);
    apply_refresh(&mut state, observation);
    assert_eq!(state.settings.monitors, committed.settings.monitors);
    assert_eq!(state.saved_monitors, committed.settings.monitors);
    assert_eq!(state.settings.appearance, committed.settings.appearance);
    assert!(!state.display_layout.is_pending());

    let _ = stop.send(());
    state.display_layout.sender = None;
    save_worker.join().unwrap();
    refresh_worker.join().unwrap();
    let (sender, mut next_requests) = tokio::sync::mpsc::unbounded_channel();
    state.display_layout.sender = Some(sender);
    save_monitor_scale(&mut state, "secondary", 2);
    assert_eq!(
        next_requests.try_recv().unwrap().monitors[0].scale,
        2,
        "a subsequent edit must not persist the pre-commit layout"
    );
}

#[test]
fn initial_get_returning_during_save_cannot_roll_back_the_committed_layout() {
    inverse_order_workers(SettingsSnapshot::new(0, Settings::default()).unwrap());
}

#[test]
fn appearance_observed_before_display_write_does_not_supersede_that_write() {
    let mut settings = Settings::default();
    settings.appearance.color_mode = hearthspace_ipc::ColorMode::Dark;
    inverse_order_workers(SettingsSnapshot::new(7, settings).unwrap());
}

#[test]
fn identical_reconnect_snapshot_after_restore_keeps_cache_until_post_commit_reconciliation() {
    let (_directory, mut state, mut receiver) = state();
    let original = SettingsSnapshot::new(0, Settings::default()).unwrap();
    apply_refresh(&mut state, (0, original.clone()));
    save_monitor_scale(&mut state, "primary", 2);
    let request = receiver.try_recv().unwrap();
    let committed = SettingsSnapshot::new(
        10,
        Settings {
            monitors: request.monitors.clone(),
            ..Settings::default()
        },
    )
    .unwrap();
    // B committed, but its acknowledgement is delayed. The original file A is
    // then restored and a reconnect GET observes its unchanged revision/cursor.
    apply_refresh(&mut state, (0, original.clone()));
    receive_display_layout(
        &mut state,
        LayoutResponse {
            request,
            saved: Ok(committed.clone()),
        },
    );
    assert_eq!(state.saved_monitors, original.settings.monitors);
    assert_eq!(
        state.preferences.snapshot.as_ref().unwrap().cursor(),
        original.cursor()
    );
    assert_eq!(state.settings.monitors[0].scale, 2);
    assert!(state.display_layout.is_pending());

    // A read started before acknowledgement cannot reconcile the pending draft.
    apply_refresh(&mut state, (0, committed));
    assert_eq!(state.saved_monitors, original.settings.monitors);
    assert_eq!(state.settings.monitors[0].scale, 2);
    assert!(state.display_layout.is_pending());
    // Only the forced post-commit GET resolves the ambiguous acknowledgement.
    apply_refresh(&mut state, (1, original.clone()));
    assert_eq!(state.settings.monitors, original.settings.monitors);
    assert_eq!(state.saved_monitors, original.settings.monitors);
    assert_eq!(
        state.preferences.snapshot.as_ref().unwrap().cursor(),
        original.cursor()
    );
    assert!(!state.display_layout.is_pending());
    assert!(
        receiver.try_recv().is_err(),
        "reconciliation must not write"
    );
    save_monitor_scale(&mut state, "secondary", 2);
    assert_eq!(
        receiver.try_recv().unwrap().monitors[0].scale,
        1,
        "the next edit must preserve the restored original layout"
    );
}

#[test]
fn post_commit_reconciliation_preserves_a_newer_local_drag() {
    let (_directory, mut state, mut receiver) = state();
    save_monitor_scale(&mut state, "primary", 2);
    let request = receiver.try_recv().unwrap();
    let committed = SettingsSnapshot::new(
        10,
        Settings {
            monitors: request.monitors.clone(),
            ..Settings::default()
        },
    )
    .unwrap();
    apply_refresh(
        &mut state,
        (0, SettingsSnapshot::new(0, Settings::default()).unwrap()),
    );
    receive_display_layout(
        &mut state,
        LayoutResponse {
            request,
            saved: Ok(committed.clone()),
        },
    );
    assert!(state.display_layout.is_pending());
    let mut draft = state.settings.monitors.clone();
    draft[1].y = 180;
    apply_monitor_editor_action(
        &mut state,
        MonitorEditorAction::Drag {
            moved_id: "secondary".into(),
            monitors: draft.clone(),
            commit: false,
        },
    );
    apply_refresh(&mut state, (1, committed.clone()));
    assert_eq!(state.saved_monitors, committed.settings.monitors);
    assert_eq!(state.settings.monitors, draft);
    assert_eq!(state.selected_monitor_id.as_deref(), Some("secondary"));
    assert!(state.display_layout.is_pending());
}
