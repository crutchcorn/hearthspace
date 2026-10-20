use hearthspace_ipc::BackgroundMode;
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, flex_col, flex_item, flex_row, label, sized_box},
};

use crate::{
    actions::{save_background_image_path, save_background_mode},
    page_components::{card, note, section},
    state::{BackgroundPickerRequest, SettingsApp},
};

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let path = state.settings.background_image_path.as_deref();
    let name = path
        .and_then(|path| std::path::Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("Default background")
        .to_string();
    let picker_label = if state.background_picker_pending {
        "Selecting background image..."
    } else {
        "Select background image"
    };
    flex_col((
        section(
            "Canvas background",
            card(
                flex_row((
                    sized_box(icon(Icon::Photo, 30.0).color(components::TEXT_MUTED))
                        .fixed_width(68.0.px())
                        .fixed_height(64.0.px())
                        .background_color(components::SURFACE_SUBTLE)
                        .corner_radius(components::RADIUS_CONTROL.px()),
                    flex_item(
                        flex_col((
                            label(name).text_size(13.0),
                            note(path.unwrap_or("The Hearthspace dot field").to_string()),
                        ))
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .gap(5.0.px()),
                        1.0,
                    ),
                ))
                .gap(12.0.px()),
            ),
        ),
        flex_col((
            components::action_button(picker_label, pick_background),
            components::action_button("Use default background", |state: &mut SettingsApp| {
                save_background_image_path(state, None);
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(6.0.px()),
        section(
            "Background fit",
            background_mode_picker(state.settings.background_mode),
        ),
        note("Choose a PNG image. Repeating follows the canvas as you move around your workspace."),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(20.0.px())
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

fn background_mode_picker(selected: BackgroundMode) -> impl WidgetView<SettingsApp> {
    let option = move |name, mode| {
        components::segmented_option(name, selected == mode, move |state: &mut SettingsApp| {
            save_background_mode(state, mode);
        })
    };
    flex_col((
        flex_row((
            option("Repeat with canvas", BackgroundMode::RepeatWithCanvas),
            option("Stretch", BackgroundMode::Stretch),
        ))
        .gap(8.0.px()),
        flex_row((
            option("Contains", BackgroundMode::Contains),
            option("Cover", BackgroundMode::Cover),
        ))
        .gap(8.0.px()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(8.0.px())
}
