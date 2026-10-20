use hearthspace_ipc::BackgroundMode;
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::{AsUnit, Dim},
        parley::style::FontWeight,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, flex_item, flex_row, label, sized_box},
};

use crate::{
    actions::save_background_image_path,
    page_components::{note, photo_fit, restore_focus_after_close, section},
    state::{BackgroundPickerRequest, SettingsApp},
};

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let path = state.settings.background_image_path.as_deref();
    let name = path
        .and_then(|path| std::path::Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("Canvas dots")
        .to_string();
    let picker_label = if state.background_picker_pending {
        "Loading photo…"
    } else if path.is_some() {
        "Change photo"
    } else {
        "Upload photo"
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
                            components::button_surface(
                                button(
                                    flex_row((
                                        icon(Icon::ImagePlus, 14.0).color(components::TEXT),
                                        label(picker_label).text_size(11.0),
                                    ))
                                    .gap(7.0.px()),
                                    pick_background,
                                )
                                .disabled(state.background_picker_pending),
                                components::ButtonSurface::Control,
                                false,
                            )
                            .height(34.0.px())
                            .padding(Padding::from_vh(7.0.px(), 10.0.px())),
                            path.is_some(),
                        ))
                        .width(Dim::MaxContent),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .gap(8.0.px()),
                    1.0,
                ),
            ))
            .gap(14.0.px()),
            path.map(|_| {
                flex_col((
                    flex_row((
                        flex_item(label("Photo fit").text_size(11.0), 1.0),
                        photo_fit(
                            state.settings.background_mode,
                            state.background_picker_pending,
                        ),
                    ))
                    .gap(12.0.px()),
                    note(
                        if state.settings.background_mode == BackgroundMode::RepeatWithCanvas {
                            "Moves with the canvas."
                        } else {
                            "Stays in place as the canvas moves."
                        },
                    ),
                    components::button_surface(
                        button(
                            flex_row((
                                icon(Icon::RotateCcw, 12.0).color(components::TEXT_MUTED),
                                label("Use default background")
                                    .text_size(10.0)
                                    .color(components::TEXT_MUTED),
                            ))
                            .gap(7.0.px()),
                            |state: &mut SettingsApp| {
                                save_background_image_path(state, None);
                            },
                        )
                        .disabled(state.background_picker_pending),
                        components::ButtonSurface::Bare,
                        false,
                    )
                    .height(32.0.px()),
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
    if state.background_picker_pending {
        state.status = "Background image picker is already open".to_string();
        return;
    }
    let Some(sender) = &state.background_picker_request_sender else {
        state.status = "Background image picker is not ready".to_string();
        return;
    };
    match sender.send(BackgroundPickerRequest::PickFile) {
        Ok(()) => {
            state.background_picker_pending = true;
            state.status = "Opening background image picker".to_string();
        }
        Err(error) => state.status = format!("Failed to open background image picker: {error}"),
    }
}

#[cfg(test)]
mod tests;
