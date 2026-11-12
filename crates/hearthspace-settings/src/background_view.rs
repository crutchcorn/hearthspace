use hearthspace_ui_components::{self as components, icons::Icon};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::{AsUnit, Dim},
        parley::style::FontWeight,
        properties::LineBreaking,
    },
    style::Style,
    view::{CrossAxisAlignment, MainAxisAlignment, flex_col, flex_item, flex_row, sized_box},
};

use crate::{
    actions::save_background_image_path,
    page_components::{photo_fit, restore_focus_after_close, section},
    state::{BackgroundPickerRequest, SettingsApp},
    typography::label,
};

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let palette = state.palette();
    let shader = state.settings.background_shader_path.is_some();
    let photo = state.settings.background_image_path.as_deref();
    let path = state.settings.background_shader_path.as_deref().or(photo);
    let name = path
        .and_then(|path| std::path::Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("Canvas dots")
        .to_string();
    let picker_label = if state.background_picker_pending {
        "Opening file…"
    } else if path.is_some() {
        "Change wallpaper"
    } else {
        "Choose wallpaper"
    };
    section(
        "Background",
        flex_col((
            flex_row((
                crate::background_preview::view(state),
                flex_item(
                    flex_col((
                        label(name)
                            .text_size(11.0)
                            .weight(FontWeight::MEDIUM)
                            .line_break_mode(LineBreaking::WordWrap),
                        sized_box(restore_focus_after_close(
                            components::controls::form_button_with_icon(
                                palette,
                                Icon::ImagePlus,
                                picker_label,
                                components::controls::ButtonVariant::Neutral,
                                state.background_picker_pending,
                                pick_background,
                            ),
                            path.is_some(),
                        ))
                        .width(Dim::MaxContent),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_alignment(MainAxisAlignment::Center)
                    .gap(8.0.px()),
                    1.0,
                ),
            ))
            .gap(14.0.px()),
            path.map(|_| {
                flex_col((
                    (!shader).then(|| {
                        flex_row((
                            flex_item(label("Photo fit").text_size(11.0), 1.0),
                            photo_fit(
                                palette,
                                state.settings.background_mode,
                                state.background_picker_pending,
                            ),
                        ))
                        .gap(12.0.px())
                    }),
                    sized_box(components::controls::form_button_with_icon(
                        palette,
                        Icon::RotateCcw,
                        "Use default background",
                        components::controls::ButtonVariant::Quiet,
                        state.background_picker_pending,
                        |state: &mut SettingsApp| save_background_image_path(state, None),
                    ))
                    .width(Dim::MaxContent),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(8.0.px())
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(15.0.px()),
    )
    .boxed()
}

fn pick_background(state: &mut SettingsApp) {
    request_background(state);
}

fn request_background(state: &mut SettingsApp) {
    if state.background_picker_pending {
        state.status = "Background picker is already open".to_string();
        return;
    }
    let Some(sender) = &state.background_picker_request_sender else {
        state.status = "Background picker is not ready".to_string();
        return;
    };
    match sender.send(BackgroundPickerRequest::PickFile) {
        Ok(()) => {
            state.background_picker_pending = true;
            state.status = "Opening background file picker".to_string();
        }
        Err(error) => state.status = format!("Failed to open background image picker: {error}"),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod shader_tests;
