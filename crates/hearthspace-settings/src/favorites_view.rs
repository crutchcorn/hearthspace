//! Saved gestures and their editor use the same real catalog and recorder.
use crate::{state::SettingsApp, typography::label};
use hearthspace_shell::gesture_pad::thumbnail_sized;
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
    view::{CrossAxisAlignment, FlexExt, flex_col, flex_row, resize_observer, sized_box},
};

mod controls;
mod editor;
mod passive;
mod picker;

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let palette = state.palette();
    let body = if state.favorites.recording.is_some() {
        editor::view(state)
    } else {
        browsing(state)
    };
    resize_observer(
        |state: &mut SettingsApp, size| state.favorites.view_width = size.width,
        flex_col((
            body,
            state
                .favorites
                .message
                .as_ref()
                .map(|message| controls::note(palette, message.clone())),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(12.0.px()),
    )
    .width(Dim::Stretch)
    .boxed()
}

fn browsing(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let palette = state.palette();
    let favorites = &state.favorites;
    let heading = flex_col((
        label("Gestures").text_size(26.0),
        label("Draw shortcuts for your apps.")
            .text_size(11.0)
            .color(palette.muted)
            .line_break_mode(LineBreaking::WordWrap),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(8.0.px());
    let add = controls::text(
        Icon::Plus,
        "Add gesture",
        favorites.store.is_none(),
        |state| state.favorites.begin_add(),
    );
    let header: Box<AnyWidgetView<SettingsApp>> = if favorites.view_width < 260.0 {
        flex_col((heading, add))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(12.0.px())
            .boxed()
    } else {
        flex_row((heading.flex(1.0), add)).gap(12.0.px()).boxed()
    };
    let rows: Vec<_> = favorites
        .assigned()
        .map(|favorite| {
            let id = favorite.app_id.clone();
            let name = favorites.name(&id);
            let installed = favorites.catalog.app_by_id(&id).is_some();
            let edit_id = id.clone();
            let remove_id = id.clone();
            let preview = components::settings_surface(
                sized_box(thumbnail_sized::<SettingsApp>(
                    favorite.gesture.as_ref().unwrap().points().to_vec(),
                    44.0,
                    40.0,
                ))
                .width(56.0.px())
                .height(48.0.px())
                .padding(Padding::from_vh(4.0.px(), 6.0.px()))
                .corner_radius(10.0.px()),
                components::SettingsSurface::Control,
            );
            let name_width = (favorites.view_width
                - if favorites.view_width < 340.0 {
                    119.0
                } else {
                    219.0
                })
            .max(1.0);
            let identity = flex_row((
                crate::favorites_icons::app_icon(state, &id, 22.0),
                label(name.clone())
                    .text_size(12.0)
                    .weight(FontWeight::MEDIUM)
                    .line_break_mode(LineBreaking::Clip)
                    .width(name_width.px()),
            ))
            .gap(9.0.px());
            let actions = flex_row((
                controls::icon_button(
                    palette,
                    Icon::Pencil,
                    format!("Change gesture for {name}"),
                    !installed,
                    move |state| state.favorites.start(&edit_id),
                ),
                controls::icon_button(
                    palette,
                    Icon::Trash2,
                    format!("Remove gesture for {name}"),
                    false,
                    move |state| state.favorites.remove_gesture(&remove_id),
                ),
            ))
            .gap(0.0.px());
            let row: Box<AnyWidgetView<SettingsApp>> = if favorites.view_width < 340.0 {
                flex_col((
                    flex_row((preview, identity.flex(1.0))).gap(12.0.px()),
                    actions,
                ))
                .cross_axis_alignment(CrossAxisAlignment::End)
                .gap(8.0.px())
                .boxed()
            } else {
                flex_row((preview, identity.flex(1.0), actions))
                    .gap(12.0.px())
                    .boxed()
            };
            components::settings_surface(
                sized_box(
                    flex_col((
                        row,
                        (!installed).then(|| controls::note(palette, "No longer installed.")),
                    ))
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .gap(4.0.px()),
                )
                .padding(8.0.px())
                .corner_radius(14.0.px()),
                components::SettingsSurface::Subtle,
            )
            .boxed()
        })
        .collect();
    let empty = rows.is_empty().then(|| {
        components::settings_surface(
            sized_box(
                flex_col((
                    icon(Icon::GestureStroke, 28.0).color(palette.muted),
                    label("No gestures yet")
                        .text_size(13.0)
                        .weight(FontWeight::MEDIUM),
                    controls::note(palette, "Add a gesture and choose the app it opens."),
                ))
                .gap(12.0.px()),
            )
            .padding(Padding::from_vh(35.0.px(), 16.0.px()))
            .corner_radius(14.0.px()),
            components::SettingsSurface::Subtle,
        )
    });
    flex_col((
        header,
        flex_col((empty, rows))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(24.0.px())
    .boxed()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod picker_tests;

pub(crate) fn theme_properties(
    properties: &mut xilem::masonry::core::DefaultProperties,
    palette: hearthspace_ui_components::Palette,
) {
    picker::theme_properties(properties, palette);
}
