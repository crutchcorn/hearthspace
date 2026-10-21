use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use rfd::FileDialog;
use xilem::{
    AnyWidgetView, WidgetView,
    core::fork,
    masonry::{
        layout::AsUnit,
        parley::style::{FontWeight, LineHeight},
        properties::Padding,
    },
    style::Style,
    view::{CrossAxisAlignment, flex_col, flex_item, flex_row, resize_observer, sized_box, worker},
};

use crate::{
    actions::save_background_image_path,
    page_components::note,
    state::{BackgroundPickerRequest, BackgroundPickerResponse, SettingsApp, SettingsCategory},
    typography::label,
};

pub(crate) fn app_logic(state: &mut SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    state
        .background_preview
        .synchronize_path(state.settings.background_image_path.as_deref());
    let content = settings_view(state);
    let content = fork(
        fork(
            fork(content, crate::activation::task()),
            crate::appearance::task(),
        ),
        crate::favorites_icons::task(),
    );
    fork(
        fork(
            fork(content, crate::ai_worker::worker_view()),
            crate::background_preview::task(),
        ),
        worker(
            |proxy, mut rx| async move {
                while let Some(BackgroundPickerRequest::PickFile) = rx.recv().await {
                    let response = match xilem::tokio::task::spawn_blocking(|| {
                        FileDialog::new()
                            .add_filter("PNG image", &["png"])
                            .pick_file()
                    })
                    .await
                    {
                        Ok(Some(path)) => BackgroundPickerResponse::Selected(path),
                        Ok(None) => BackgroundPickerResponse::Cancelled,
                        Err(error) => BackgroundPickerResponse::Failed(error.to_string()),
                    };

                    drop(proxy.message(response));
                }
            },
            |state: &mut SettingsApp, sender| {
                state.background_picker_request_sender = Some(sender);
            },
            |state: &mut SettingsApp, response| {
                state.background_picker_pending = false;
                match response {
                    BackgroundPickerResponse::Selected(path) => {
                        save_background_image_path(state, Some(path));
                    }
                    BackgroundPickerResponse::Cancelled => {
                        state.status = "Background image selection cancelled".to_string();
                    }
                    BackgroundPickerResponse::Failed(error) => {
                        state.status = format!("Background image picker failed: {error}");
                    }
                }
            },
        ),
    )
}

pub(crate) mod navigation;

pub(crate) fn settings_view(state: &SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    let palette = state.palette();
    resize_observer(
        |state: &mut SettingsApp, size| state.navigation.width = size.width,
        crate::window_frame::body(
            navigation::view(
                sized_box(category_view(state)).background_color(palette.surface_subtle),
                sidebar_view(state),
                state,
            ),
            state.settings.background_image_path.is_some(),
        ),
    )
}

fn sidebar_view(state: &SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    let palette = state.palette();
    let mut navigation_palette = palette;
    if state.navigation.compact() {
        navigation_palette.button_hover = palette.raised_button_hover;
    }
    let items = [
        (SettingsCategory::Appearance, Icon::Appearance),
        (SettingsCategory::Displays, Icon::Monitor),
        (SettingsCategory::Background, Icon::Photo),
        (SettingsCategory::Gestures, Icon::GestureStroke),
        (SettingsCategory::Ai, Icon::Processor),
        (SettingsCategory::System, Icon::Sliders),
    ]
    .map(|(category, glyph)| {
        let selected = state.selected_category == category;
        let tint = if selected {
            palette.accent_decorative
        } else {
            palette.muted
        };
        components::settings_button(
            crate::page_components::disclosure(
                category.label(),
                false,
                false,
                flex_row((
                    icon(glyph, 16.0).color(tint),
                    flex_item(
                        label(category.label()).text_size(11.0).color(if selected {
                            palette.text
                        } else {
                            palette.muted
                        }),
                        1.0,
                    ),
                ))
                .gap(10.0.px()),
                move |state: &mut SettingsApp| state.select_category(category),
            )
            .current_page(selected)
            .corner_radius(components::RADIUS_CONTROL.px())
            .height(40.0.px())
            .border_width(1.0.px())
            .padding(Padding::from_vh(9.0.px(), 10.0.px())),
            components::SettingsButton::Bare,
            selected,
        )
        .prop(navigation_palette)
    });
    components::scroll_area(
        sized_box(
            flex_col(items)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(5.0.px()),
        )
        .padding(if state.navigation.compact() {
            Padding::all(8.0.px())
        } else {
            Padding::from_vh(22.0.px(), 12.0.px())
        }),
    )
}

fn category_view(state: &SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    let palette = state.palette();
    let category = state.selected_category;
    components::scroll_area(
        sized_box(
            flex_col((
                (category != SettingsCategory::Gestures).then(|| {
                    flex_col((
                        label(category.label())
                            .text_size(26.0)
                            .letter_spacing(-0.7)
                            .line_height(LineHeight::FontSizeRelative(1.1))
                            .weight(FontWeight::NORMAL),
                        note(palette, category.description()),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .gap(8.0.px())
                }),
                selected_category_view(state),
                status_footer(state),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(if category == SettingsCategory::Displays {
                22.0.px()
            } else {
                25.0.px()
            }),
        )
        .width(xilem::masonry::layout::Dim::Stretch)
        .padding(Padding::from_vh(23.0.px(), 24.0.px())),
    )
}

fn selected_category_view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    match state.selected_category {
        SettingsCategory::Appearance => crate::appearance_view::view(state),
        SettingsCategory::Displays => crate::display_view::view(state),
        SettingsCategory::Background => crate::background_view::view(state),
        SettingsCategory::Gestures => crate::favorites_view::view(state),
        SettingsCategory::Ai => crate::ai_view::view(state),
        SettingsCategory::System => crate::system_view::view(state),
    }
}

fn status_footer(state: &SettingsApp) -> Option<impl WidgetView<SettingsApp> + use<>> {
    let palette = state.palette();
    (!matches!(
        state.status.as_str(),
        "" | "Connected to hearthspace-settingsd" | "Connected to the settings service"
    ))
    .then(|| note(palette, state.status.clone()))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod navigation_tests;
