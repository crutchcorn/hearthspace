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
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, flex_col, flex_item, flex_row, resize_observer, sized_box, worker},
};

use crate::{
    actions::{save_background_image_path, save_background_shader_path},
    page_components::note,
    state::{BackgroundPickerResponse, SettingsApp, SettingsCategory},
    typography::label,
};

pub(crate) fn app_logic(state: &mut SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    state
        .background_preview
        .synchronize_path(state.settings.background_image_path.as_deref());
    state
        .background_shader_status
        .synchronize_path(state.settings.background_shader_path.as_deref());
    let content = settings_view(state);
    let content = fork(content, crate::background_shader_status::task());
    let content = fork(content, crate::frame_settings::task());
    let content = fork(content, crate::settings_backend::task());
    let content = fork(content, crate::wayland_outputs::task());
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
                while let Some(request) = rx.recv().await {
                    let _ = request;
                    let response = match xilem::tokio::task::spawn_blocking(move || {
                        let Some(path) = FileDialog::new()
                            .add_filter(
                                "Wallpaper",
                                &["png", "jpg", "jpeg", "PNG", "JPG", "JPEG", "glsl", "frag"],
                            )
                            .pick_file()
                        else {
                            return BackgroundPickerResponse::Cancelled;
                        };
                        let shader = path.extension().and_then(|v| v.to_str()).is_some_and(|v| {
                            v.eq_ignore_ascii_case("glsl") || v.eq_ignore_ascii_case("frag")
                        });
                        if shader {
                            match crate::background_shader_status::load_preview(&path) {
                                Ok(image) => BackgroundPickerResponse::ShaderSelected(path, image),
                                Err(_) => BackgroundPickerResponse::Failed(
                                    "Wallpaper failed to load".into(),
                                ),
                            }
                        } else {
                            let mut cache = crate::background_thumbnail::ThumbnailCache::new();
                            if cache.load(&path).is_some() {
                                BackgroundPickerResponse::Selected(path)
                            } else {
                                BackgroundPickerResponse::Failed("Wallpaper failed to load".into())
                            }
                        }
                    })
                    .await
                    {
                        Ok(response) => response,
                        Err(_) => {
                            BackgroundPickerResponse::Failed("Wallpaper failed to load".into())
                        }
                    };

                    drop(proxy.message(response));
                }
            },
            |state: &mut SettingsApp, sender| {
                state.background_picker_request_sender = Some(sender);
            },
            background_picker_response,
        ),
    )
}

pub(crate) fn background_picker_response(
    state: &mut SettingsApp,
    response: BackgroundPickerResponse,
) {
    state.background_picker_pending = false;
    crate::settings_refresh::local_save_finished(state);
    match response {
        BackgroundPickerResponse::ShaderSelected(path, image) => {
            let selected = path.to_str().map(str::to_owned);
            save_background_shader_path(state, Some(path));
            if state.settings.background_shader_path == selected {
                state
                    .background_shader_status
                    .synchronize_path(selected.as_deref());
                state.background_shader_status.image = Some(image);
            }
        }
        BackgroundPickerResponse::Selected(path) => save_background_image_path(state, Some(path)),
        BackgroundPickerResponse::Cancelled => state.status.clear(),
        BackgroundPickerResponse::Failed(error) => {
            let _ = error;
            state.status = "Wallpaper failed to load".into()
        }
    }
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
            state.settings.has_custom_background(),
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
        (SettingsCategory::KeyboardShortcuts, Icon::List),
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
        let title = label(category.label()).text_size(11.0);
        let title = if category == SettingsCategory::KeyboardShortcuts {
            title.line_height(LineHeight::FontSizeRelative(1.0))
        } else {
            title
        };
        let title = title
            .color(if selected {
                palette.text
            } else {
                palette.muted
            })
            .line_break_mode(if category == SettingsCategory::KeyboardShortcuts {
                LineBreaking::WordWrap
            } else {
                LineBreaking::Overflow
            });
        components::settings_button(
            crate::page_components::disclosure(
                category.label(),
                false,
                false,
                flex_row((icon(glyph, 16.0).color(tint), flex_item(title, 1.0))).gap(10.0.px()),
                move |state: &mut SettingsApp| state.select_category(category),
            )
            .current_page(selected)
            .corner_radius(components::RADIUS_CONTROL.px())
            .height(40.0.px())
            .border_width(1.0.px())
            .padding(Padding::from_vh(
                if category == SettingsCategory::KeyboardShortcuts {
                    8.0.px()
                } else {
                    9.0.px()
                },
                10.0.px(),
            )),
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
        SettingsCategory::KeyboardShortcuts => crate::shortcuts_view::view(state),
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
pub(crate) mod tests;

#[cfg(test)]
mod navigation_tests;

#[cfg(test)]
mod navigation_focus_tests;

#[cfg(test)]
mod system_tests;

#[cfg(test)]
mod shortcuts_tests;
