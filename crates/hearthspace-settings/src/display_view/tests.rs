use crate::{
    monitors::{LiveOutput, settings_with_live_outputs},
    state::{SettingsApp, SettingsCategory},
    test_support::{Ui, ids},
};
use hearthspace_ipc::Settings;
use masonry::accesskit::Role;
use xilem::{WidgetView, view::sized_box};

fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(super::view(state))
}

#[test]
fn display_identity_keeps_product_names_and_shows_each_connector_once_without_position() {
    let (_directory, favorites) = crate::favorites::tests::fixture();
    let live_outputs: Vec<_> = [
        ("eDP-1", "DRM - eDP-1 - eDP-1"),
        ("DP-1", "DELL - U2723QE - DP-1"),
        ("HDMI-A-1", "DRM - HDMI-A-1 - HDMI-A-1"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (id, name))| LiveOutput {
        id: id.into(),
        name: name.into(),
        width: 1920,
        height: 1080,
        scale: 1,
        refresh_millihertz: [Some(60_000), Some(59_940), None][index],
    })
    .collect();
    let settings = settings_with_live_outputs(Settings::default(), &live_outputs);
    let state = SettingsApp {
        activation_listener: None,
        ai: Default::default(),
        favorites,
        settings,
        live_outputs,
        selected_category: SettingsCategory::Displays,
        selected_monitor_id: None,
        background_preview: Default::default(),
        background_picker_request_sender: None,
        background_picker_pending: false,
        status: String::new(),
    };
    let mut ui = Ui::new(state, view, (440, 450));
    for (index, name, connector) in [
        (0, "Built-in display", "eDP-1"),
        (1, "DELL U2723QE", "DP-1"),
        (2, "External display", "HDMI-A-1"),
    ] {
        ui.state.selected_monitor_id = Some(ui.state.settings.monitors[index].id.clone());
        ui.rebuild(&view);
        ui.dispatch(&view);
        let labels: Vec<_> = ids(&ui.harness)
            .into_iter()
            .filter_map(|id| {
                let node = ui.harness.access_node(id)?;
                (node.role() == Role::Label).then(|| node.value()).flatten()
            })
            .collect();
        assert!(labels.iter().any(|label| label == name), "{labels:?}");
        assert!(
            labels
                .iter()
                .any(|label| label == &format!("Display {} · {connector}", index + 1)),
            "{labels:?}"
        );
        assert_eq!(
            labels
                .iter()
                .filter(|label| label.contains(connector))
                .count(),
            1,
            "the connector should appear once: {labels:?}"
        );
        let refresh = ["60 Hz", "59.94 Hz", "Not reported"][index];
        let refresh_node = ui.harness.access_node(ui.button(refresh)).unwrap();
        assert!(
            refresh_node.is_disabled(),
            "refresh is reported, not editable"
        );
        assert!(
            labels.iter().all(|label| !label.starts_with("Position:")),
            "desktop coordinates should not be a visible settings row"
        );
    }
}

#[test]
fn scale_popup_keeps_page_geometry_and_saves_the_keyboard_choice_to_the_backend() {
    use hearthspace_ipc::SettingsRequest;
    use masonry::core::{
        TextEvent,
        keyboard::{Key, NamedKey},
    };
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
        process::Command,
    };

    const CHILD: &str = "HEARTHSPACE_TEST_DISPLAY_SELECT";
    if std::env::var_os(CHILD).is_none() {
        let directory = tempfile::tempdir().unwrap();
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "display_view::tests::scale_popup_keeps_page_geometry_and_saves_the_keyboard_choice_to_the_backend",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env(hearthspace_common::SETTINGS_SOCKET_ENV, directory.path().join("settings.sock"))
            .env(hearthspace_common::SHELL_COMMAND_SOCKET_ENV, directory.path().join("no-compositor.sock"))
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
        let mut settings = Settings::default();
        // A scale update and the two existing monitor positions use the real IPC path.
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = String::new();
            BufReader::new(&stream).read_line(&mut request).unwrap();
            match hearthspace_ipc::parse_settings_request(&request).unwrap() {
                SettingsRequest::SetMonitorScale { id, scale } => {
                    settings
                        .monitors
                        .iter_mut()
                        .find(|monitor| monitor.id == id)
                        .unwrap()
                        .scale = scale;
                }
                SettingsRequest::SetMonitorPosition { id, x, y } => {
                    let monitor = settings
                        .monitors
                        .iter_mut()
                        .find(|monitor| monitor.id == id)
                        .unwrap();
                    monitor.x = x;
                    monitor.y = y;
                }
                request => panic!("unexpected display request: {request:?}"),
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
        settings: Settings::default(),
        live_outputs: vec![],
        selected_category: SettingsCategory::Displays,
        selected_monitor_id: None,
        background_preview: Default::default(),
        background_picker_request_sender: None,
        background_picker_pending: false,
        status: String::new(),
    };
    let mut ui = Ui::new(state, view, (470, 490));
    let select = ids(&ui.harness)
        .into_iter()
        .find(|&id| {
            ui.harness.access_node(id).is_some_and(|node| {
                node.role() == Role::ComboBox && node.label().as_deref() == Some("Scale")
            })
        })
        .unwrap();
    let refresh = ui.button("Not reported");
    let before = ui.harness.get_widget_with_id(refresh).ctx().bounding_box();
    ui.harness.focus_on(Some(select));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    let mut nodes = vec![ui.harness.access_tree().state().root()];
    let mut options = vec![];
    while let Some(node) = nodes.pop() {
        nodes.extend(node.children());
        if node.role() == Role::ListBoxOption {
            options.push(node.label().unwrap());
        }
    }
    options.sort();
    assert_eq!(options, ["100%", "200%", "300%", "400%"]);
    assert_eq!(
        ui.harness.get_widget_with_id(refresh).ctx().bounding_box(),
        before,
        "the scale menu floats without moving the settings below it"
    );
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert_eq!(ui.state.settings.monitors[0].scale, 2);
    assert_eq!(
        ui.harness.access_node(select).unwrap().value().as_deref(),
        Some("200%")
    );
    assert_eq!(
        ui.harness.access_node(select).unwrap().data().is_expanded(),
        Some(false)
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(select));
    let saved = backend.join().unwrap();
    assert_eq!(saved.monitors[0].scale, 2);
    assert_eq!(saved.monitors[1].x, saved.monitors[0].width / 2);
}

#[test]
fn monitor_preview_keeps_its_reference_aspect_when_settings_content_narrows() {
    let (_directory, favorites) = crate::favorites::tests::fixture();
    let state = SettingsApp {
        activation_listener: None,
        ai: Default::default(),
        favorites,
        settings: Settings::default(),
        live_outputs: vec![],
        selected_category: SettingsCategory::Displays,
        selected_monitor_id: None,
        background_preview: Default::default(),
        background_picker_request_sender: None,
        background_picker_pending: false,
        status: String::new(),
    };
    let mut ui = Ui::new(state, view, (470, 490));
    for width in [470, 300] {
        ui.harness
            .process_window_event(masonry::core::WindowEvent::Resize((width, 490).into()));
        ui.harness.redraw();
        let id = ids(&ui.harness)
            .into_iter()
            .find(|&id| {
                ui.harness
                    .access_node(id)
                    .is_some_and(|node| node.role() == Role::Canvas)
            })
            .unwrap();
        let preview = ui.harness.get_widget_with_id(id).ctx().bounding_box();
        assert!(
            (preview.width() / preview.height() - 800.0 / 450.0).abs() < 0.01,
            "{preview:?}"
        );
        assert!(
            preview.width() <= 400.0 && preview.x0 >= 0.0 && preview.x1 <= f64::from(width),
            "{preview:?}"
        );
    }
}
