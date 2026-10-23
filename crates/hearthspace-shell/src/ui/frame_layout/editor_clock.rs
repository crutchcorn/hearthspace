//! Layout editing retains the same shared clock choices and session guard.
use super::ShellState;
use hearthspace_common::frame_layout::ClockDateFormat;
use hearthspace_ui_components::Palette;
use xilem::{
    WidgetView,
    masonry::{layout::AsUnit, properties::Padding},
    style::Style,
    view::{CrossAxisAlignment, flex_col, label},
};

pub(super) fn view(
    selected: ClockDateFormat,
    palette: Palette,
) -> impl WidgetView<ShellState> + use<> {
    flex_col((
        label("Date format")
            .font("Liberation Sans")
            .text_size(11.0)
            .color(palette.muted)
            .padding(Padding {
                top: 8.0.px(),
                bottom: 4.0.px(),
                left: 10.0.px(),
                right: 10.0.px(),
            }),
        crate::ui::clock_format::choices(selected, palette, None, |state, format| {
            super::command(
                state,
                "clock-date-format",
                serde_json::json!({"value":format}),
            );
        }),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(0.0.px())
    .padding(Padding::from_vh(4.0.px(), 0.0.px()))
}
