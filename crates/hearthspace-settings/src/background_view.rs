use hearthspace_ipc::BackgroundMode;
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::{AsUnit, Dim},
        parley::style::{FontWeight, LineHeight},
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{
        CrossAxisAlignment, MainAxisAlignment, button, flex_col, flex_item, flex_row, sized_box,
    },
};

use crate::{
    actions::save_background_image_path,
    page_components::{photo_fit, restore_focus_after_close, section, select},
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
    } else if shader {
        "Change GLSL file"
    } else if path.is_some() {
        "Change photo"
    } else {
        "Upload photo"
    };
    section(
        "Background",
        flex_col((
            flex_row((
                flex_item(label("Wallpaper type").text_size(11.0), 1.0),
                sized_box(select(
                    palette, "Wallpaper type", vec!["Canvas dots".into(), "Photo".into(), "GLSL".into()],
                    if shader { 2 } else if photo.is_some() { 1 } else { 0 },
                    state.background_picker_pending,
                    |state, index| match index {
                        0 => save_background_image_path(state, None),
                        1 => request_background(state, false),
                        _ => request_background(state, true),
                    },
                )).fixed_width(134.0.px()).fixed_height(34.0.px()),
            )),
            flex_row((
                crate::background_preview::view(state),
                flex_item(
                    flex_col((
                        label(name)
                            .text_size(11.0)
                            .weight(FontWeight::MEDIUM)
                            .line_break_mode(LineBreaking::WordWrap),
                        sized_box(restore_focus_after_close(
                            components::settings_button(
                                button(
                                    flex_row((
                                        icon(Icon::ImagePlus, 14.0).color(palette.text),
                                        label(picker_label).text_size(11.0),
                                    ))
                                    .gap(7.0.px()),
                                    pick_background,
                                )
                                .height(34.0.px())
                                .padding(Padding::from_vh(7.0.px(), 10.0.px())),
                                components::SettingsButton::Control,
                                false,
                            )
                            .disabled(state.background_picker_pending),
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
                    shader.then(|| components::settings_button(
                        button(label("Reload GLSL file").text_size(11.0), |state: &mut SettingsApp| state.background_shader_status.reload()),
                        components::SettingsButton::Control, false,
                    )),
                    shader.then(|| label(state.background_shader_status.message.clone()).text_size(10.0).line_break_mode(LineBreaking::WordWrap).color(if state.background_shader_status.failed { palette.text } else { palette.muted })),
                    (!shader).then(|| flex_row((
                        flex_item(label("Photo fit").text_size(11.0), 1.0),
                        photo_fit(
                            palette,
                            state.settings.background_mode,
                            state.background_picker_pending,
                        ),
                    ))
                    .gap(12.0.px())),
                    sized_box(
                        label(
                            if shader {
                                "GLSL wallpaper runs on the desktop. Invalid shaders fall back to Canvas dots."
                            } else if state.settings.background_mode == BackgroundMode::RepeatWithCanvas {
                                "Moves with the canvas."
                            } else {
                                "Stays in place as the canvas moves."
                            },
                        )
                        .text_size(10.0)
                        .line_height(LineHeight::FontSizeRelative(1.5))
                        .color(palette.muted),
                    )
                    .padding(Padding::bottom(4.0.px())),
                    sized_box(
                        components::settings_button(
                            button(
                                flex_row((
                                    icon(Icon::RotateCcw, 12.0).color(palette.text),
                                    label("Use default background")
                                        .text_size(10.0)
                                        .color(palette.text),
                                ))
                                .gap(7.0.px()),
                                |state: &mut SettingsApp| {
                                    save_background_image_path(state, None);
                                },
                            )
                            .height(32.0.px())
                            .padding(Padding::from_vh(7.0.px(), 10.0.px())),
                            components::SettingsButton::Outline,
                            false,
                        )
                        .disabled(state.background_picker_pending),
                    )
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
    request_background(state, state.settings.background_shader_path.is_some());
}

fn request_background(state: &mut SettingsApp, shader: bool) {
    if state.background_picker_pending {
        state.status = "Background picker is already open".to_string();
        return;
    }
    let Some(sender) = &state.background_picker_request_sender else {
        state.status = "Background picker is not ready".to_string();
        return;
    };
    match sender.send(if shader {
        BackgroundPickerRequest::PickShader
    } else {
        BackgroundPickerRequest::PickFile
    }) {
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
