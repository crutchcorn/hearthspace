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
    .map(|(id, name)| LiveOutput {
        id: id.into(),
        name: name.into(),
        width: 1920,
        height: 1080,
        scale: 1,
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
        assert!(
            labels.iter().all(|label| !label.starts_with("Position:")),
            "desktop coordinates should not be a visible settings row"
        );
    }
}
