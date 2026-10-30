//! A read-only reference for fixed canvas keyboard and pointer shortcuts.
use hearthspace_ipc::shortcuts::CANVAS_SHORTCUTS;
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

use crate::{
    page_components::{note, section},
    state::SettingsApp,
    typography::label,
};

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let palette = state.palette();
    let keyboard_rows = CANVAS_SHORTCUTS
        .iter()
        .map(|shortcut| {
            row(
                palette,
                shortcut.label,
                shortcut.chord(),
                shortcut.description,
            )
        })
        .collect();
    let pointer_rows = vec![
        row(
            palette,
            "Temporarily pan canvas",
            "❖ + Left-drag".into(),
            "Hold ❖ before left-dragging empty canvas to pan without changing the selected tool. Panning ends when you release the mouse button, even if you release ❖ first.",
        ),
        row(
            palette,
            "Zoom canvas",
            "❖ + Scroll".into(),
            "Hold ❖ and scroll to zoom the canvas.",
        ),
    ];
    flex_col((
        note(palette, "These shortcuts cannot be changed yet."),
        section(
            "Keyboard",
            flex_col((
                note(palette, "❖ means Meta (Super, Windows, or Command). Hold it and press a key. You can release both keys after choosing a tool."),
                rows(keyboard_rows),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(12.0.px()),
        ),
        section(
            "Pointer shortcuts",
            flex_col((
                note(palette, "Hold ❖ for these pointer gestures; they do not select a tool."),
                rows(pointer_rows),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(12.0.px()),
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(18.0.px())
    .boxed()
}

fn rows(rows: Vec<impl WidgetView<SettingsApp>>) -> impl WidgetView<SettingsApp> {
    components::settings_surface(
        sized_box(
            flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(0.0.px()),
        )
        .corner_radius(components::RADIUS_TILE.px()),
        components::SettingsSurface::Subtle,
    )
}

fn row(
    palette: Palette,
    name: &'static str,
    chord: String,
    description: &'static str,
) -> impl WidgetView<SettingsApp> + use<> {
    sized_box(
        flex_col((
            flex_row((
                label(name)
                    .text_size(12.0)
                    .weight(FontWeight::MEDIUM)
                    .color(palette.text)
                    .line_break_mode(LineBreaking::WordWrap)
                    .flex(1.0),
                sized_box(label(chord).text_size(11.0).color(palette.text))
                    .padding(Padding::from_vh(3.0.px(), 7.0.px()))
                    .corner_radius(components::RADIUS_DETAIL.px())
                    .border_width(1.0.px())
                    .border_color(palette.border)
                    .background_color(palette.surface_control),
            ))
            .gap(12.0.px()),
            note(palette, description),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(6.0.px()),
    )
    .padding(12.0.px())
}
