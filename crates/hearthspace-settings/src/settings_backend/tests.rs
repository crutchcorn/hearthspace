use super::*;
use crate::{
    actions::{MonitorEditorAction, apply_monitor_editor_action, save_monitor_scale},
    state::SettingsCategory,
};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

fn state() -> (
    tempfile::TempDir,
    SettingsApp,
    UnboundedReceiver<LayoutRequest>,
) {
    let (directory, favorites) = crate::favorites::tests::fixture();
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    let state = SettingsApp {
        running: true,
        navigation: Default::default(),
        preferences: Default::default(),
        appearance_theme: Default::default(),
        activation_listener: None,
        ai: Default::default(),
        favorites,
        frame_settings: Default::default(),
        display_layout: DisplayLayoutState {
            sender: Some(sender),
            ..Default::default()
        },
        settings: Settings::default(),
        saved_monitors: hearthspace_ipc::Settings::default().monitors,
        output_observation: Default::default(),
        live_outputs: vec![],
        selected_category: SettingsCategory::Displays,
        selected_monitor_id: Some("primary".into()),
        background_preview: Default::default(),
        background_picker_request_sender: None,
        background_picker_pending: false,
        status: String::new(),
    };
    (directory, state, receiver)
}

fn successful(request: LayoutRequest) -> LayoutResponse {
    LayoutResponse {
        saved: Ok(SettingsSnapshot::new(
            request.revision,
            Settings {
                monitors: request.monitors.clone(),
                ..Settings::default()
            },
        )
        .unwrap()),
        request,
    }
}

#[test]
fn late_save_cannot_replace_a_newer_drag_preview_or_selection() {
    let (_directory, mut state, mut receiver) = state();
    save_monitor_scale(&mut state, "primary", 2);
    let first = receiver.try_recv().unwrap();
    let mut preview = state.settings.monitors.clone();
    preview[1].y = 400;
    apply_monitor_editor_action(
        &mut state,
        MonitorEditorAction::Drag {
            moved_id: "secondary".into(),
            monitors: preview.clone(),
            commit: false,
        },
    );
    receive_display_layout(&mut state, successful(first));
    assert_eq!(state.settings.monitors, preview);
    assert_eq!(state.selected_monitor_id.as_deref(), Some("secondary"));
    assert!(state.display_layout.is_pending());
    assert_eq!(state.status, "");
}

#[test]
fn consecutive_edits_ignore_older_results_and_preserve_other_preferences() {
    let (_directory, mut state, mut receiver) = state();
    save_monitor_scale(&mut state, "primary", 2);
    let first = receiver.try_recv().unwrap();
    save_monitor_scale(&mut state, "primary", 3);
    let second = receiver.try_recv().unwrap();
    state.settings.background_image_path = Some("newer #image.png".into());
    state.settings.appearance.color_mode = hearthspace_ipc::ColorMode::Dark;
    state.settings.test = true;
    state.selected_monitor_id = Some("secondary".into());

    receive_display_layout(&mut state, successful(first));
    assert_eq!(state.settings.monitors[0].scale, 3);
    assert!(state.display_layout.is_pending());
    let latest = successful(second);
    receive_display_layout(&mut state, latest);
    assert_eq!(state.settings.monitors[0].scale, 3);
    assert_eq!(
        state.settings.background_image_path.as_deref(),
        Some("newer #image.png")
    );
    assert_eq!(
        state.settings.appearance.color_mode,
        hearthspace_ipc::ColorMode::Dark
    );
    assert!(state.settings.test);
    assert_eq!(state.selected_monitor_id.as_deref(), Some("secondary"));
    assert!(!state.display_layout.is_pending());
    assert!(state.status.is_empty());
}

#[test]
fn failed_save_keeps_the_preview_and_does_not_overwrite_another_pages_status() {
    let (_directory, mut state, mut receiver) = state();
    save_monitor_scale(&mut state, "primary", 2);
    let request = receiver.try_recv().unwrap();
    state.status = "Saved appearance".into();
    receive_display_layout(
        &mut state,
        LayoutResponse {
            request,
            saved: Err("not connected".into()),
        },
    );
    assert_eq!(state.settings.monitors[0].scale, 2);
    assert!(state.display_layout.is_pending());
    assert_eq!(state.status, "Saved appearance");
}

#[test]
fn save_completion_after_mode_change_and_selected_removal_updates_preferences_not_connections() {
    use crate::{
        monitors::LiveOutput,
        wayland_outputs::{Snapshot, apply_snapshot},
    };
    let (_directory, mut state, mut receiver) = state();
    let output = |id: &str, width| LiveOutput {
        id: id.into(),
        name: id.into(),
        width,
        height: 1080,
        scale: 1,
        refresh_millihertz: Some(60_000),
    };
    apply_snapshot(
        &mut state,
        Snapshot {
            outputs: vec![output("eDP-1", 1920), output("DP-1", 1920)],
            primary: Some("eDP-1".into()),
        },
    );
    save_monitor_scale(&mut state, "eDP-1", 2);
    let request = receiver.try_recv().unwrap();
    state.selected_monitor_id = Some("DP-1".into());
    apply_snapshot(
        &mut state,
        Snapshot {
            outputs: vec![output("eDP-1", 2560)],
            primary: Some("eDP-1".into()),
        },
    );
    assert_eq!(state.selected_monitor_id.as_deref(), Some("eDP-1"));
    assert_eq!(state.settings.monitors[0].width, 2560);
    receive_display_layout(&mut state, successful(request));
    assert!(!state.display_layout.is_pending());
    assert_eq!(state.settings.monitors.len(), 1);
    assert_eq!(state.settings.monitors[0].width, 2560);
    assert_eq!(state.settings.monitors[0].scale, 2);
    assert!(state.saved_monitors.iter().any(|row| row.id == "DP-1"));
    apply_snapshot(
        &mut state,
        Snapshot {
            outputs: vec![output("DP-1", 1920), output("eDP-1", 2560)],
            primary: Some("eDP-1".into()),
        },
    );
    assert_eq!(state.settings.monitors.len(), 2);
    assert_eq!(state.settings.monitors[0].id, "eDP-1");
    assert_eq!(state.settings.monitors[0].scale, 2);
}

#[test]
fn serial_worker_commits_complete_layouts_without_a_compositor_connection() {
    let directory = tempfile::tempdir().unwrap();
    let settings_path = directory.path().join("settings.sock");
    let command_path = directory.path().join("command.sock");
    let settings_listener = UnixListener::bind(&settings_path).unwrap();
    settings_listener.set_nonblocking(true).unwrap();
    let command_listener = UnixListener::bind(&command_path).unwrap();
    command_listener.set_nonblocking(true).unwrap();
    let (_state_directory, mut state, receiver) = state();
    let (responses, received) = mpsc::channel();
    let worker = thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run_worker(
                receiver,
                Endpoints {
                    settings: Ok(settings_path),
                },
                move |response| responses.send(response).is_ok(),
            ));
    });

    save_monitor_scale(&mut state, "primary", 2);
    save_monitor_scale(&mut state, "primary", 3);
    let mut first = accept(&settings_listener);
    let first_layout = read_layout(&first);
    assert_eq!(first_layout[0].scale, 2);
    assert_eq!(first_layout[1].x, 960);
    assert_eq!(
        settings_listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    respond(&mut first, &first_layout);
    let first_result = received.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(first_result.saved.is_ok());
    assert_eq!(
        command_listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    receive_display_layout(&mut state, first_result);
    assert_eq!(state.settings.monitors[0].scale, 3);

    let mut second = accept(&settings_listener);
    let second_layout = read_layout(&second);
    assert_eq!(second_layout[0].scale, 3);
    assert_eq!(second_layout[1].x, 640);
    respond(&mut second, &second_layout);
    let second_result = received.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(second_result.saved.is_ok());
    receive_display_layout(&mut state, second_result);
    assert!(!state.display_layout.is_pending());
    assert_eq!(state.status, "");
    state.display_layout.sender = None;
    worker.join().unwrap();
}

#[test]
fn rejected_save_does_not_request_a_reload_or_emit_rollback_writes() {
    let directory = tempfile::tempdir().unwrap();
    let settings_path = directory.path().join("settings.sock");
    let command_path = directory.path().join("command.sock");
    let settings_listener = UnixListener::bind(&settings_path).unwrap();
    settings_listener.set_nonblocking(true).unwrap();
    let command_listener = UnixListener::bind(&command_path).unwrap();
    command_listener.set_nonblocking(true).unwrap();
    let (_state_directory, mut state, mut receiver) = state();
    save_monitor_scale(&mut state, "primary", 2);
    let request = receiver.try_recv().unwrap();
    let work = thread::spawn(move || {
        execute(
            request,
            Endpoints {
                settings: Ok(settings_path),
            },
        )
    });
    let mut stream = accept(&settings_listener);
    assert_eq!(read_layout(&stream)[0].scale, 2);
    stream
        .write_all(b"err cannot persist preferences\n")
        .unwrap();
    let response = work.join().unwrap();
    assert!(response.saved.is_err());
    assert_eq!(
        command_listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        settings_listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    receive_display_layout(&mut state, response);
    assert!(state.status.contains("cannot persist preferences"));
    assert_eq!(state.settings.monitors[0].scale, 2);
}

fn accept(listener: &UnixListener) -> UnixStream {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                return stream;
            }
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("worker did not connect: {error}"),
        }
    }
}

fn read_line(stream: &UnixStream) -> String {
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    line
}

fn read_layout(stream: &UnixStream) -> Vec<MonitorConfig> {
    match hearthspace_ipc::parse_settings_request(
        read_line(stream).strip_prefix("commit ").unwrap(),
    )
    .unwrap()
    {
        hearthspace_ipc::SettingsRequest::SetDisplayLayout(monitors) => monitors,
        other => panic!("expected one complete layout, got {other:?}"),
    }
}

fn respond(stream: &mut UnixStream, monitors: &[MonitorConfig]) {
    let saved = Settings {
        monitors: monitors.to_vec(),
        ..Settings::default()
    };
    stream
        .write_all(
            hearthspace_ipc::settings_snapshot::snapshot_response(Some(
                SettingsSnapshot::new(monitors[0].scale as u64, saved).unwrap(),
            ))
            .unwrap()
            .as_bytes(),
        )
        .unwrap();
}

#[test]
fn completion_cannot_replace_a_later_external_display_commit() {
    let (_directory, mut state, mut receiver) = state();
    save_monitor_scale(&mut state, "primary", 2);
    let request = receiver.try_recv().unwrap();
    let mut external = Settings::default();
    external.monitors[0].scale = 3;
    external.monitors[1].x = 640;
    let saved = external.monitors.clone();
    let snapshot = SettingsSnapshot::new(20, external).unwrap();
    crate::settings_refresh::apply_refresh(&mut state, (0, snapshot.clone()));
    assert_eq!(state.settings.monitors[0].scale, 2);
    assert_eq!(state.saved_monitors, saved);
    receive_display_layout(&mut state, successful(request));
    assert_eq!(state.settings.monitors[0].scale, 2);
    assert!(state.display_layout.is_pending());
    crate::settings_refresh::apply_refresh(&mut state, (1, snapshot));
    assert_eq!(state.settings.monitors, saved);
    assert!(!state.display_layout.is_pending());
}

mod review;

mod reconciliation;
