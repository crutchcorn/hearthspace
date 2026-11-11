use crate::{
    state::{BackgroundPickerRequest, BackgroundPickerResponse, SettingsCategory},
    test_support::{Ui, ids},
};
use xilem::{masonry::accesskit::Role, tokio::sync::mpsc::unbounded_channel};

#[test]
fn one_picker_keeps_the_previous_wallpaper_when_loading_fails() {
    let (_dir, mut state) = crate::ui::tests::state();
    state.selected_category = SettingsCategory::Background;
    state.settings.background_shader_path = Some("working.glsl".into());
    let (sender, mut receiver) = unbounded_channel();
    state.background_picker_request_sender = Some(sender);
    let mut ui = Ui::new(state, super::view, (400, 540));
    ui.activate(ui.button("Change wallpaper"));
    ui.dispatch(&super::view);
    assert!(matches!(
        receiver.try_recv().unwrap(),
        BackgroundPickerRequest::PickFile
    ));
    assert!(ui.state.background_picker_pending);
    crate::ui::background_picker_response(
        &mut ui.state,
        BackgroundPickerResponse::Failed("compiler details must stay hidden".into()),
    );
    assert_eq!(
        ui.state.settings.background_shader_path.as_deref(),
        Some("working.glsl")
    );
    assert!(ui.state.settings.background_image_path.is_none());
    assert_eq!(ui.state.status, "Wallpaper failed to load");
}

#[test]
fn shader_page_has_no_type_selector_reload_or_developer_status() {
    let (_dir, mut state) = crate::ui::tests::state();
    state.selected_category = SettingsCategory::Background;
    state.settings.background_shader_path = Some("working.glsl".into());
    let ui = Ui::new(state, super::view, (400, 540));
    ui.button("Change wallpaper");
    ui.button("Use default background");
    for id in ids(&ui.harness) {
        if let Some(node) = ui.harness.access_node(id) {
            assert_ne!(node.role(), Role::ComboBox);
            let text = node.label().or_else(|| node.value()).unwrap_or_default();
            assert!(
                !text.contains("Reload GLSL")
                    && !text.contains("active on the desktop")
                    && !text.contains("Invalid shaders")
            );
        }
    }
}
