use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use rfd::FileDialog;
use xilem::{
    AnyWidgetView, WidgetView,
    core::fork,
    masonry::{layout::AsUnit, parley::style::FontWeight, properties::Padding},
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, flex_item, flex_row, label, sized_box, worker},
};

use crate::{
    actions::save_background_image_path,
    page_components::note,
    state::{BackgroundPickerRequest, BackgroundPickerResponse, SettingsApp, SettingsCategory},
};

pub(crate) fn app_logic(state: &mut SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    let content = settings_view(state);
    fork(
        fork(content, crate::ai_worker::worker_view()),
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

pub(crate) fn settings_view(state: &SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    sized_box(
        flex_row((
            sized_box(sidebar_view(state))
                .fixed_width(159.0.px())
                .background_color(components::SURFACE_SUBTLE),
            sized_box(label(""))
                .fixed_width(1.0.px())
                .background_color(components::BORDER),
            flex_item(category_view(state), 1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(0.0.px()),
    )
    .background_color(components::SURFACE)
}

fn sidebar_view(state: &SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
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
            components::ACCENT_STRONG
        } else {
            components::TEXT_MUTED
        };
        components::button_surface(
            button(
                flex_row((
                    icon(glyph, 16.0).color(tint),
                    flex_item(
                        label(category.label()).text_size(12.0).color(if selected {
                            components::TEXT
                        } else {
                            components::TEXT_MUTED
                        }),
                        1.0,
                    ),
                ))
                .gap(10.0.px()),
                move |state: &mut SettingsApp| state.select_category(category),
            ),
            components::ButtonSurface::Bare,
            selected,
        )
        .corner_radius(components::RADIUS_CONTROL.px())
        .padding(10.0.px())
    });
    components::scroll_area(
        sized_box(
            flex_col(items)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(5.0.px()),
        )
        .padding(Padding::from_vh(22.0.px(), 12.0.px())),
    )
}

fn category_view(state: &SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    let category = state.selected_category;
    components::scroll_area(
        sized_box(
            flex_col((
                flex_col((
                    label(category.label())
                        .text_size(24.0)
                        .weight(FontWeight::MEDIUM),
                    note(category.description()),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(9.0.px()),
                selected_category_view(state),
                status_footer(state),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(24.0.px()),
        )
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

fn status_footer(state: &SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    note(state.status.clone())
}

#[cfg(test)]
mod tests;
