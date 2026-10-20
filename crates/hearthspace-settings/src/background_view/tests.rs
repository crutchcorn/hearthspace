use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    process::Command,
};

use crate::{
    state::{SettingsApp, SettingsCategory},
    test_support::{Ui, ids},
};
use hearthspace_ipc::{BackgroundMode, Settings, SettingsRequest};
use xilem::{
    WidgetView,
    masonry::{
        accesskit::Role,
        core::{
            TextEvent,
            keyboard::{Key, NamedKey},
        },
    },
    view::sized_box,
};

fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(super::view(state))
}

#[test]
fn photo_fit_selection_and_reset_keep_keyboard_focus() {
    const CHILD: &str = "HEARTHSPACE_TEST_BACKGROUND_FOCUS";
    // Isolate the real IPC client from the user's settings daemon without
    // mutating environment variables alongside parallel tests.
    if std::env::var_os(CHILD).is_none() {
        let directory = tempfile::tempdir().unwrap();
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "background_view::tests::photo_fit_selection_and_reset_keep_keyboard_focus",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env(
                hearthspace_common::SETTINGS_SOCKET_ENV,
                directory.path().join("settings.sock"),
            )
            .env(
                hearthspace_common::SHELL_COMMAND_SOCKET_ENV,
                directory.path().join("no-compositor.sock"),
            )
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let listener = UnixListener::bind(hearthspace_ipc::settings_socket_path().unwrap()).unwrap();
    let settings = Settings {
        background_image_path: Some("photo.png".into()),
        background_mode: BackgroundMode::RepeatWithCanvas,
        ..Settings::default()
    };
    let server_settings = settings.clone();
    let backend = std::thread::spawn(move || {
        let mut settings = server_settings;
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = String::new();
            BufReader::new(&stream).read_line(&mut request).unwrap();
            match hearthspace_ipc::parse_settings_request(&request).unwrap() {
                SettingsRequest::SetBackgroundMode(mode) => settings.background_mode = mode,
                SettingsRequest::SetBackgroundImagePath(path) => {
                    settings.background_image_path = path
                }
                request => panic!("unexpected request: {request:?}"),
            }
            stream
                .write_all(hearthspace_ipc::settings_response(settings.clone()).as_bytes())
                .unwrap();
        }
        settings
    });
    let (_directory, favorites) = crate::favorites::tests::fixture();
    let state = SettingsApp {
        activation_listener: None,
        ai: Default::default(),
        favorites,
        settings,
        live_outputs: vec![],
        selected_category: SettingsCategory::Background,
        selected_monitor_id: None,
        background_picker_request_sender: None,
        background_picker_pending: false,
        background_fit_open: false,
        status: String::new(),
        config_label: String::new(),
    };
    let mut ui = Ui::new(state, view, (440, 420));
    let upload = ui.button("Change photo");
    assert_ne!(
        ui.harness.focused_widget_id(),
        Some(upload),
        "no initial autofocus"
    );
    let opener = ui.button("Photo fit: Repeat with canvas");
    ui.harness.focus_on(Some(opener));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness
            .access_node(id)
            .is_some_and(|node| node.role() == Role::RadioGroup)
    }));
    let cover = ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.role() == Role::RadioButton
                    && node.label().as_deref() == Some("Photo fit: Cover")
            })
        })
        .unwrap();
    ui.harness.focus_on(Some(cover));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    ui.dispatch(&view);
    assert_eq!(ui.state.settings.background_mode, BackgroundMode::Cover);
    assert_eq!(ui.harness.focused_widget_id(), Some(cover));
    assert!(ui.state.background_fit_open);
    ui.harness
        .focus_on(Some(ui.button("Use default background")));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert_eq!(
        ui.button("Upload photo"),
        upload,
        "picker identity stays stable"
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(upload));
    assert!(ui.state.settings.background_image_path.is_none());
    let saved = backend.join().unwrap();
    assert_eq!(saved.background_mode, BackgroundMode::Cover);
    assert!(saved.background_image_path.is_none());
}
