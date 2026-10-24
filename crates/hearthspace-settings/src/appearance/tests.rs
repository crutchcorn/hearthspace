use super::*;
use crate::{state::SettingsCategory, test_support::Ui};
use hearthspace_ipc::SettingsRequest;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    process::Command,
    sync::Arc,
};
use xilem::{
    WidgetView,
    masonry::{
        accesskit::Toggled,
        core::{
            TextEvent,
            keyboard::{Key, NamedKey},
            pointer::PointerButton,
        },
    },
    view::sized_box,
};

fn state() -> (tempfile::TempDir, SettingsApp) {
    let (directory, favorites) = crate::favorites::tests::fixture();
    (
        directory,
        SettingsApp {
            running: true,
            preferences: Default::default(),
            appearance_theme: Default::default(),
            navigation: Default::default(),
            frame_settings: Default::default(),
            display_layout: Default::default(),
            activation_listener: None,
            ai: Default::default(),
            favorites,
            settings: Settings::default(),
            saved_monitors: hearthspace_ipc::Settings::default().monitors,
            output_observation: Default::default(),
            live_outputs: vec![],
            selected_category: SettingsCategory::Appearance,
            selected_monitor_id: None,
            background_preview: Default::default(),
            background_picker_request_sender: None,
            background_picker_pending: false,
            status: String::new(),
        },
    )
}

fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(crate::appearance_view::view(state))
}

#[test]
fn keyboard_and_pointer_choices_save_through_settings_ipc_without_remounting() {
    const CHILD: &str = "HEARTHSPACE_TEST_APPEARANCE";
    if std::env::var_os(CHILD).is_none() {
        let directory = tempfile::tempdir().unwrap();
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "appearance::tests::keyboard_and_pointer_choices_save_through_settings_ipc_without_remounting",
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
    let backend = std::thread::spawn(move || {
        let mut saved = Settings::default();
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = String::new();
            BufReader::new(&stream).read_line(&mut request).unwrap();
            match hearthspace_ipc::parse_settings_request(&request).unwrap() {
                SettingsRequest::SetColorMode(mode) => saved.appearance.color_mode = mode,
                SettingsRequest::SetAccentColor(accent) => saved.appearance.accent_color = accent,
                SettingsRequest::Get => {}
                request => panic!("unexpected appearance request: {request:?}"),
            }
            stream
                .write_all(hearthspace_ipc::settings_response(saved.clone()).as_bytes())
                .unwrap();
        }
        saved
    });
    let (_directory, mut state) = state();
    state.settings.background_image_path = Some("pending-local-photo.png".into());
    let mut ui = Ui::new(state, view, (470, 440));
    let dark = ui.button("Dark");
    let blue = ui.button("Blue");
    ui.harness.focus_on(Some(dark));
    for event in [
        TextEvent::key_down(Key::Named(NamedKey::Enter)),
        TextEvent::key_up(Key::Named(NamedKey::Enter)),
    ] {
        ui.harness.process_text_event(event);
    }
    ui.dispatch(&view);
    assert_eq!(ui.state.settings.appearance.color_mode, ColorMode::Dark);
    assert_eq!(ui.harness.focused_widget_id(), Some(dark));
    assert_eq!(ui.button("Dark"), dark);
    assert_eq!(
        ui.harness.access_node(dark).unwrap().toggled(),
        Some(Toggled::True)
    );
    let control = ui.harness.get_widget_with_id(blue);
    let center = control
        .ctx()
        .window_transform()
        .transform_rect_bbox(control.ctx().border_box())
        .center();
    ui.harness.mouse_move(center);
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert_eq!(ui.state.settings.appearance.accent_color, AccentColor::Blue);
    assert_eq!(ui.button("Blue"), blue);
    assert_eq!(ui.harness.focused_widget_id(), Some(blue));
    assert_eq!(
        ui.harness.access_node(blue).unwrap().toggled(),
        Some(Toggled::True)
    );
    assert_eq!(
        ui.state.settings.background_image_path.as_deref(),
        Some("pending-local-photo.png"),
        "saving appearance must not overwrite unrelated local state"
    );
    assert_eq!(
        client_get_settings().unwrap().appearance,
        ui.state.settings.appearance
    );
    assert_eq!(
        backend.join().unwrap().appearance,
        ui.state.settings.appearance
    );
}

#[test]
fn refresh_ignores_a_read_queued_before_save_and_errors_keep_the_previous_theme() {
    let (_directory, mut state) = state();
    let before = state.settings.appearance;
    saved(
        &mut state,
        Err(std::io::Error::other("service unavailable")),
    );
    assert_eq!(state.settings.appearance, before);
    assert!(state.status.contains("Failed to save appearance"));
    state.settings.appearance = Appearance {
        color_mode: ColorMode::Dark,
        accent_color: AccentColor::Teal,
    };
    crate::settings_refresh::local_save_finished(&mut state);
    let snapshot = |appearance| {
        hearthspace_ipc::settings_snapshot::SettingsSnapshot::new(
            1,
            Settings {
                appearance,
                ..Settings::default()
            },
        )
        .unwrap()
    };
    crate::settings_refresh::apply_refresh(&mut state, (0, snapshot(before)));
    assert_eq!(state.settings.appearance.accent_color, AccentColor::Teal);
    let external = Appearance {
        color_mode: ColorMode::Light,
        accent_color: AccentColor::Violet,
    };
    crate::settings_refresh::apply_refresh(&mut state, (1, snapshot(external)));
    assert_eq!(state.settings.appearance, external);
}

#[test]
fn default_properties_are_reused_until_appearance_or_photo_material_changes() {
    let theme = Theme::default();
    let appearance = Appearance::default();
    let original = theme.properties(appearance, false);
    assert!(Arc::ptr_eq(&original, &theme.properties(appearance, false)));
    let mut changed = appearance;
    changed.accent_color = AccentColor::Rose;
    let accent = theme.properties(changed, false);
    assert!(!Arc::ptr_eq(&original, &accent));
    assert!(Arc::ptr_eq(&accent, &theme.properties(changed, false)));
    assert!(!Arc::ptr_eq(&accent, &theme.properties(changed, true)));
}
