use crate::{
    monitors::{LiveOutput, monitors_for_live_outputs},
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
        (
            "eDP-1@pci-internal",
            "DRM - eDP-1 - eDP-1 (eDP-1@pci-internal)",
        ),
        ("DP-1@pci-a", "DELL - U2723QE - DP-1"),
        ("DP-1@pci-b", "DELL - U2723QE - DP-1"),
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
        refresh_millihertz: [Some(60_000), Some(59_940), Some(120_000), None][index],
    })
    .collect();
    let settings = Settings {
        monitors: monitors_for_live_outputs(
            &Settings::default().monitors,
            &live_outputs,
            Some("eDP-1@pci-internal"),
        ),
        ..Settings::default()
    };
    let state = SettingsApp {
        running: true,
        preferences: Default::default(),
        appearance_theme: Default::default(),
        navigation: Default::default(),
        frame_settings: Default::default(),
        display_layout: Default::default(),
        activation_listener: None,
        ai: Default::default(),
        favorites,
        settings,
        saved_monitors: hearthspace_ipc::Settings::default().monitors,
        output_observation: Default::default(),
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
        (2, "DELL U2723QE", "DP-1"),
        (3, "External display", "HDMI-A-1"),
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
        let refresh = ["60 Hz", "59.94 Hz", "120 Hz", "Not reported"][index];
        let refresh_node = ui.harness.access_node(ui.button(refresh)).unwrap();
        assert!(
            refresh_node.is_disabled(),
            "refresh is reported, not editable"
        );
        assert!(
            labels.iter().all(|label| !label.starts_with("Position:")),
            "desktop coordinates should not be a visible settings row"
        );
        assert!(
            labels.iter().all(|label| !label.contains('@')),
            "{labels:?}"
        );
        let canvas = ids(&ui.harness)
            .into_iter()
            .find_map(|id| {
                let node = ui.harness.access_node(id)?;
                (node.role() == Role::Canvas)
                    .then(|| node.description())
                    .flatten()
            })
            .unwrap();
        assert!(
            canvas.starts_with(&format!("Display {}: {name}.", index + 1)),
            "{canvas}"
        );
        assert!(
            !canvas.contains('@'),
            "GPU routing IDs are not accessible labels"
        );
    }
}

#[test]
fn scale_popup_keeps_page_geometry_and_queues_the_keyboard_choice() {
    use masonry::core::{
        TextEvent,
        keyboard::{Key, NamedKey},
    };
    let (sender, mut receiver) = xilem::tokio::sync::mpsc::unbounded_channel();
    let (_directory, favorites) = crate::favorites::tests::fixture();
    let state = SettingsApp {
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
        selected_category: SettingsCategory::Displays,
        selected_monitor_id: None,
        background_preview: Default::default(),
        background_picker_request_sender: None,
        background_picker_pending: false,
        status: String::new(),
    };
    let mut ui = Ui::new(state, view, (470, 490));
    ui.state.display_layout.sender = Some(sender);
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
    let request = receiver.try_recv().unwrap();
    assert_eq!(request.monitors[0].scale, 2);
    assert_eq!(request.monitors[1].x, request.monitors[0].width / 2);
    assert!(
        receiver.try_recv().is_err(),
        "one complete transaction per scale edit"
    );
}

#[test]
fn monitor_preview_keeps_its_reference_aspect_when_settings_content_narrows() {
    let (_directory, favorites) = crate::favorites::tests::fixture();
    let state = SettingsApp {
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
