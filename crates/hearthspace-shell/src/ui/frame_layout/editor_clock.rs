//! Clock choices use one native radio target for the full reference row.
use super::{ShellState, widgets};
use hearthspace_common::frame_layout::ClockDateFormat;
use hearthspace_ui_components::{self as components, Palette};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        imaging::Painter,
        kurbo::{Circle, Stroke},
        layout::AsUnit,
        parley::style::LineHeight,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{
        CrossAxisAlignment, FlexExt, canvas, flex_col, flex_row, label, radio_group, sized_box,
    },
};

pub(super) fn view(
    selected: ClockDateFormat,
    palette: Palette,
) -> impl WidgetView<ShellState> + use<> {
    let rows: Vec<Box<AnyWidgetView<ShellState>>> = [
        (ClockDateFormat::None, "Time only"),
        (ClockDateFormat::Weekday, "Weekday"),
        (ClockDateFormat::Short, "Short date"),
        (ClockDateFormat::Numeric, "Numeric date"),
        (ClockDateFormat::Long, "Long date"),
    ]
    .into_iter()
    .map(|(format, title)| {
        let checked = selected == format;
        let indicator = sized_box(canvas(move |_: &mut ShellState, _, scene, _| {
            let mut painter = Painter::new(scene);
            let color = if checked {
                palette.accent
            } else {
                palette.muted
            };
            painter
                .fill(Circle::new((7.0, 7.0), 6.5), palette.surface_control)
                .draw();
            painter
                .stroke(Circle::new((7.0, 7.0), 6.0), &Stroke::new(1.0), color)
                .draw();
            if checked {
                painter
                    .fill(Circle::new((7.0, 7.0), 4.0), palette.accent)
                    .draw();
            }
        }))
        .width(14.0.px())
        .height(14.0.px());
        // The browser's Linux sans-serif resolves to Liberation Sans; its
        // default digits are tabular, keeping these live time samples aligned.
        let text = |text| {
            label(text)
                .font("Liberation Sans")
                .text_size(12.0)
                .line_height(LineHeight::FontSizeRelative(1.4))
                .line_break_mode(LineBreaking::WordWrap)
        };
        let content = flex_row((
            indicator,
            flex_col((
                text(title.to_string()).color(if checked {
                    palette.accent
                } else {
                    palette.text
                }),
                text(widgets::clock(format)).color(palette.muted),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(3.0.px())
            .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(10.0.px());
        let choice = components::semantic_radio_button(
            content,
            title,
            checked,
            move |state: &mut ShellState| {
                super::command(
                    state,
                    "clock-date-format",
                    serde_json::json!({"value":format}),
                );
            },
        )
        .height(52.0.px())
        .padding(Padding::from_vh(8.0.px(), 10.0.px()))
        .corner_radius(15.0.px())
        .border_width(0.0.px());
        crate::ui::canvas_search::material::choice(choice, checked, palette).boxed()
    })
    .collect();
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
        radio_group(
            flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(0.0.px()),
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(0.0.px())
    .padding(Padding::from_vh(4.0.px(), 0.0.px()))
}
