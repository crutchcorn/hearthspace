use super::{BackgroundPreview, Response};
use crate::{
    state::{SettingsApp, SettingsCategory},
    test_support::Ui,
};
use hearthspace_ipc::Settings;
use xilem::{
    WidgetView,
    masonry::peniko::{ImageAlphaType, ImageBrush, ImageData, ImageFormat},
    view::sized_box,
};

fn settings_view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(crate::ui::settings_view(state))
}

fn image(red: u8) -> ImageBrush {
    ImageData {
        data: vec![red, 0, 0, 255].into(),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: 1,
        height: 1,
    }
    .into()
}

fn red(preview: &BackgroundPreview, path: &str) -> Option<u8> {
    preview
        .image_for(Some(path))
        .map(|brush| brush.image.data.as_ref()[0])
}

#[test]
fn late_thumbnail_cannot_replace_a_newer_selection_or_restore_a_reset_image() {
    let mut preview = BackgroundPreview::default();
    preview.refresh(Some("first.png"));
    let first = preview.request.clone();
    preview.refresh(Some("second.png"));
    let second = preview.request.clone();
    preview.receive(Response {
        request: first,
        image: Some(image(10)),
    });
    assert!(preview.image_for(Some("second.png")).is_none());
    preview.receive(Response {
        request: second.clone(),
        image: Some(image(20)),
    });
    assert_eq!(red(&preview, "second.png"), Some(20));

    preview.refresh(None);
    assert!(preview.image.is_none());
    preview.receive(Response {
        request: second,
        image: Some(image(20)),
    });
    assert!(
        preview.image.is_none(),
        "reset must remain the dotted background"
    );
}

#[test]
fn reselecting_the_same_path_rejects_an_older_decode_and_keeps_the_ready_preview() {
    let mut preview = BackgroundPreview::default();
    preview.refresh(Some("photo.png"));
    let first = preview.request.clone();
    preview.receive(Response {
        request: first.clone(),
        image: Some(image(10)),
    });
    preview.refresh(Some("photo.png"));
    assert_eq!(red(&preview, "photo.png"), Some(10));
    let refreshed = preview.request.clone();
    preview.receive(Response {
        request: refreshed,
        image: Some(image(20)),
    });
    preview.receive(Response {
        request: first,
        image: Some(image(10)),
    });
    assert_eq!(red(&preview, "photo.png"), Some(20));
}

#[test]
fn a_failed_refresh_removes_the_stale_image_without_changing_the_selected_path() {
    let mut preview = BackgroundPreview::default();
    preview.refresh(Some("photo.png"));
    preview.receive(Response {
        request: preview.request.clone(),
        image: Some(image(10)),
    });
    preview.refresh(Some("photo.png"));
    preview.receive(Response {
        request: preview.request.clone(),
        image: None,
    });
    assert_eq!(preview.request.path.as_deref(), Some("photo.png"));
    assert!(preview.image.is_none());
}

#[test]
fn category_navigation_works_while_loading_and_reentry_keeps_the_cached_preview() {
    let (_directory, favorites) = crate::favorites::tests::fixture();
    let mut state = SettingsApp {
        running: true,
        appearance_theme: Default::default(),
        navigation: Default::default(),
        frame_settings: Default::default(),
        activation_listener: None,
        ai: Default::default(),
        favorites,
        settings: Settings {
            background_image_path: Some("/missing/photo.png".into()),
            ..Settings::default()
        },
        live_outputs: vec![],
        selected_category: SettingsCategory::Background,
        selected_monitor_id: None,
        background_preview: Default::default(),
        background_picker_request_sender: None,
        background_picker_pending: false,
        status: String::new(),
    };
    state
        .background_preview
        .synchronize_path(state.settings.background_image_path.as_deref());
    let request = state.background_preview.request.clone();
    let mut ui = Ui::new(state, settings_view, (680, 490));
    ui.activate(ui.button("Displays"));
    ui.dispatch(&settings_view);
    assert_eq!(ui.state.selected_category, SettingsCategory::Displays);
    // Completing in another category warms the preview without changing pages.
    ui.state.background_preview.receive(Response {
        request,
        image: Some(image(30)),
    });
    ui.rebuild(&settings_view);
    assert_eq!(ui.state.selected_category, SettingsCategory::Displays);
    ui.activate(ui.button("Background"));
    ui.dispatch(&settings_view);
    assert_eq!(ui.state.selected_category, SettingsCategory::Background);
    assert_eq!(
        red(&ui.state.background_preview, "/missing/photo.png"),
        Some(30)
    );
    assert!(
        !ui.harness
            .access_node(ui.button("Use default background"))
            .unwrap()
            .is_disabled()
    );
}
