//! A read-only view of the same fixed canvas bindings used by input and shell hints.
use hearthspace_ipc::shortcuts::{CANVAS_SHORTCUTS, CanvasShortcut};
use hearthspace_ui_components::{self as components, Palette};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::AsUnit,
        parley::style::FontWeight,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, FlexExt, flex_col, flex_row, sized_box},
};

use crate::{page_components::note, state::SettingsApp, typography::label};

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let palette = state.palette();
    let rows: Vec<_> = CANVAS_SHORTCUTS
        .iter()
        .map(|shortcut| row(palette, shortcut))
        .collect();
    flex_col((
        flex_col((
            note(palette, "Hold Meta (Super, Windows, or Command) and press a key. You can release both keys after choosing a tool."),
            note(palette, "These shortcuts cannot be changed yet."),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(8.0.px()),
        components::settings_surface(
            sized_box(
                flex_col(rows)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .gap(0.0.px()),
            )
            .corner_radius(components::RADIUS_TILE.px()),
            components::SettingsSurface::Subtle,
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(18.0.px())
    .boxed()
}

fn row(palette: Palette, shortcut: &CanvasShortcut) -> impl WidgetView<SettingsApp> + use<> {
    sized_box(
        flex_col((
            flex_row((
                label(shortcut.label)
                    .text_size(12.0)
                    .weight(FontWeight::MEDIUM)
                    .color(palette.text)
                    .line_break_mode(LineBreaking::WordWrap)
                    .flex(1.0),
                sized_box(label(shortcut.chord()).text_size(11.0).color(palette.text))
                    .padding(Padding::from_vh(3.0.px(), 7.0.px()))
                    .corner_radius(components::RADIUS_DETAIL.px())
                    .border_width(1.0.px())
                    .border_color(palette.border)
                    .background_color(palette.surface_control),
            ))
            .gap(12.0.px()),
            note(palette, shortcut.description),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(6.0.px()),
    )
    .padding(12.0.px())
}
