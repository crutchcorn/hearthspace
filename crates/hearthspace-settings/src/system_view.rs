use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::AsUnit,
        parley::style::LineHeight,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, flex_row},
};

use crate::{
    page_components::{section, toggle_row},
    state::SettingsApp,
    typography::label,
};

pub(crate) fn view(_state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    flex_col((
        section(
            "Frame layout",
            flex_col((
                label("Arrange and group your frame items, or change how much detail they show.")
                    .text_size(12.0)
                    .line_height(LineHeight::FontSizeRelative(1.5))
                    .line_break_mode(LineBreaking::WordWrap)
                    .color(components::settings_material::MUTED),
                unavailable_action("Change layout", Icon::FrameLayout, 15.0),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(8.0.px()),
        ),
        section(
            "Canvas tools",
            flex_col((
                toggle_row(
                    "Make canvas tools hide-able",
                    Some("Hiding canvas tools is not available yet."),
                    false,
                    true,
                    |_, _| {},
                )
                .height(75.0.px()),
                unavailable_action("Restore canvas tools", Icon::RotateCcw, 12.0),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(27.0.px())
    .boxed()
}
fn unavailable_action(name: &'static str, glyph: Icon, size: f64) -> impl WidgetView<SettingsApp> {
    components::settings_button(
        button(
            flex_row((
                icon(glyph, size).color(components::settings_material::MUTED),
                label(name)
                    .text_size(10.0)
                    .color(components::settings_material::MUTED),
            ))
            .gap(7.0.px()),
            |_: &mut SettingsApp| {},
        )
        .height(36.0.px())
        .padding(Padding::from_vh(8.0.px(), 10.0.px())),
        components::SettingsButton::Bare,
        false,
    )
    .disabled(true)
}
