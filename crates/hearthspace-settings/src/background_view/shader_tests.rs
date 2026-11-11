use crate::{
    state::{BackgroundPickerRequest, BackgroundPickerResponse, SettingsCategory},
    test_support::{Ui, ids},
};
use hearthspace_ipc::ColorMode;
use xilem::{
    masonry::{
        accesskit::Role,
        core::{
            TextEvent,
            keyboard::{Key, NamedKey},
        },
    },
    tokio::sync::mpsc::unbounded_channel,
};

#[test]
fn glsl_type_uses_shader_picker_and_cancellation_preserves_the_photo() {
    let (_dir, mut state) = crate::ui::tests::state();
    state.selected_category = SettingsCategory::Background;
    state.settings.background_image_path = Some("original.png".into());
    let (sender, mut receiver) = unbounded_channel();
    state.background_picker_request_sender = Some(sender);
    let mut ui = Ui::new(state, super::view, (340, 460));
    let selector = ids(&ui.harness)
        .into_iter()
        .find(|&id| {
            ui.harness.access_node(id).is_some_and(|node| {
                node.role() == Role::ComboBox && node.label().as_deref() == Some("Wallpaper type")
            })
        })
        .expect("Wallpaper type selector");
    ui.harness.focus_on(Some(selector));
    for key in [NamedKey::Enter, NamedKey::End, NamedKey::Enter] {
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(key)));
        ui.harness
            .process_text_event(TextEvent::key_up(Key::Named(key)));
        ui.dispatch(&super::view);
    }
    assert!(matches!(
        receiver.try_recv().unwrap(),
        BackgroundPickerRequest::PickShader
    ));
    assert!(ui.state.background_picker_pending);
    assert!(ui.harness.access_node(selector).unwrap().is_disabled());
    assert_eq!(
        ui.state.settings.background_image_path.as_deref(),
        Some("original.png")
    );
    assert!(ui.state.settings.background_shader_path.is_none());
    crate::ui::background_picker_response(&mut ui.state, BackgroundPickerResponse::Cancelled);
    ui.rebuild(&super::view);
    ui.dispatch(&super::view);
    assert!(!ui.state.background_picker_pending);
    assert_eq!(
        ui.state.settings.background_image_path.as_deref(),
        Some("original.png")
    );
    assert_eq!(
        ui.harness.access_node(selector).unwrap().value().as_deref(),
        Some("Photo")
    );
    assert!(!ui.harness.access_node(selector).unwrap().is_disabled());
}

#[test]
fn glsl_controls_expose_reload_and_errors_without_photo_fit_in_both_themes() {
    for mode in [ColorMode::Light, ColorMode::Dark] {
        for width in [320, 680] {
            let (_dir, mut state) = crate::ui::tests::state();
            state.selected_category = SettingsCategory::Background;
            state.settings.appearance.color_mode = mode;
            state.settings.background_shader_path = Some("/wallpapers/aurora.glsl".into());
            state
                .background_shader_status
                .synchronize_path(Some("/wallpapers/aurora.glsl"));
            state.background_shader_status.message =
                "Syntax error: showing Canvas dots instead.".into();
            state.background_shader_status.failed = true;
            let mut ui = Ui::new(state, super::view, (width, 540));
            assert!(
                ids(&ui.harness)
                    .into_iter()
                    .any(|id| ui.harness.access_node(id).is_some_and(|node| node
                        .label()
                        .or_else(|| node.value())
                        .is_some_and(|label| label.contains("Syntax error"))))
            );
            assert!(!ids(&ui.harness).into_iter().any(|id| {
                ui.harness.access_node(id).is_some_and(|node| {
                    node.role() == Role::ComboBox && node.label().as_deref() == Some("Photo fit")
                })
            }));
            for name in [
                "Change GLSL file",
                "Reload GLSL file",
                "Use default background",
            ] {
                let id = ui.button(name);
                let widget = ui.harness.get_widget_with_id(id);
                let bounds = widget
                    .ctx()
                    .window_transform()
                    .transform_rect_bbox(widget.ctx().border_box());
                assert!(
                    bounds.x0 >= 0. && bounds.x1 <= f64::from(width),
                    "{name}: {bounds:?}"
                );
            }
            let reload = ui.button("Reload GLSL file");
            ui.harness.focus_on(Some(reload));
            ui.activate(reload);
            ui.dispatch(&super::view);
            assert_eq!(ui.harness.focused_widget_id(), Some(reload));
            assert!(!ui.state.background_shader_status.failed);
            assert_eq!(
                ui.state.background_shader_status.message,
                "Applying GLSL wallpaper…"
            );
        }
    }
}
