use super::controls;
use crate::state::SettingsApp;
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::AsUnit,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, flex_row, label, sized_box},
};

pub(super) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let favorites = &state.favorites;
    let record = favorites.recording.as_ref().unwrap();
    let name = favorites.name(&record.app_id);
    let field = crate::page_components::disclosure(
        format!("{name} Choose app"),
        favorites.picker_open,
        false,
        flex_row((
            crate::favorites_icons::app_icon(state, &record.app_id, 24.0),
            label(name)
                .text_size(12.0)
                .line_break_mode(LineBreaking::Clip)
                .width((favorites.view_width - 88.0).max(1.0).px()),
            icon(
                if favorites.picker_open {
                    Icon::ChevronUp
                } else {
                    Icon::ChevronDown
                },
                16.0,
            )
            .color(components::TEXT_MUTED),
        ))
        .gap(10.0.px()),
        |state: &mut SettingsApp| state.favorites.picker_open = !state.favorites.picker_open,
    )
    .height(44.0.px())
    .padding(Padding::from_vh(0.0.px(), 12.0.px()))
    .background_color(components::SURFACE_CONTROL)
    .border(components::CHROME_BORDER, 1.0.px())
    .corner_radius(10.0.px());
    let field = crate::page_components::restore_focus_after_close(field, favorites.picker_open);
    let picker = favorites.picker_open.then(|| {
        let apps: Vec<_> = favorites
            .catalog
            .search(&favorites.query, 48)
            .into_iter()
            .map(|app| {
                let id = app.id.clone();
                components::button_surface(
                    button(
                        flex_row((
                            crate::favorites_icons::app_icon(state, &app.id, 24.0),
                            label(app.name)
                                .text_size(12.0)
                                .line_break_mode(LineBreaking::Clip)
                                .width((favorites.view_width - 88.0).max(1.0).px()),
                        ))
                        .gap(10.0.px()),
                        move |state: &mut SettingsApp| state.favorites.choose_app(&id),
                    ),
                    components::ButtonSurface::Bare,
                    record.app_id == app.id,
                )
                .height(44.0.px())
                .padding(Padding::from_vh(0.0.px(), 10.0.px()))
                .corner_radius(8.0.px())
                .boxed()
            })
            .collect();
        let empty = apps.is_empty();
        sized_box(
            flex_col((
                crate::favorites_search::input(&favorites.query),
                empty.then(|| controls::note("No matching apps.")),
                sized_box(components::scroll_area(
                    flex_col(apps)
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .gap(2.0.px()),
                ))
                .height(176.0.px()),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
        )
        .padding(8.0.px())
        .background_color(components::SURFACE_CONTROL)
        .border(components::CHROME_BORDER, 1.0.px())
        .corner_radius(10.0.px())
    });
    flex_col((controls::note("Open this app"), field, picker))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(7.0.px())
        .boxed()
}
