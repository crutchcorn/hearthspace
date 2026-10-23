//! Shared clock formatting and native choice rows for the editor and calendar.
use super::{
    ShellState,
    search_focus::{FocusTarget, Record},
};
use hearthspace_common::frame_layout::ClockDateFormat;
use hearthspace_ui_components::{self as components, Palette};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        imaging::Painter,
        kurbo::{Circle, Stroke},
        layout::{AsUnit, Dim},
        parley::style::LineHeight,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{
        CrossAxisAlignment, FlexExt, canvas, flex_col, flex_row, label, radio_group, sized_box,
    },
};

pub(super) const FORMATS: [(ClockDateFormat, &str); 5] = [
    (ClockDateFormat::None, "Time only"),
    (ClockDateFormat::Weekday, "Weekday"),
    (ClockDateFormat::Short, "Short date"),
    (ClockDateFormat::Numeric, "Numeric date"),
    (ClockDateFormat::Long, "Long date"),
];

pub(super) fn index(format: ClockDateFormat) -> usize {
    FORMATS
        .iter()
        .position(|(candidate, _)| *candidate == format)
        .unwrap()
}

pub(in crate::ui) fn preview(format: ClockDateFormat) -> String {
    let now = chrono::Local::now();
    let date = match format {
        ClockDateFormat::None => return now.format("%H:%M").to_string(),
        ClockDateFormat::Weekday => "%a",
        ClockDateFormat::Short => "%b %-d",
        ClockDateFormat::Numeric => "%m/%d/%Y",
        ClockDateFormat::Long => "%B %-d, %Y",
    };
    format!("{} · {}", now.format(date), now.format("%H:%M"))
}

pub(super) fn choices<F>(
    selected: ClockDateFormat,
    palette: Palette,
    targets: Option<&[FocusTarget; 5]>,
    on_select: F,
) -> impl WidgetView<ShellState> + use<F>
where
    F: Fn(&mut ShellState, ClockDateFormat) + Clone + Send + Sync + 'static,
{
    let rows: Vec<Box<AnyWidgetView<ShellState>>> = FORMATS
        .into_iter()
        .enumerate()
        .map(|(index, (format, title))| {
            let on_select = on_select.clone();
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
            .fixed_width(14.0.px())
            .fixed_height(14.0.px());
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
                    text(preview(format)).color(palette.muted),
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
                move |state: &mut ShellState| on_select(state, format),
            )
            .height(if targets.is_some() {
                Dim::Auto
            } else {
                Dim::Fixed(52.0.px())
            })
            .padding(Padding::from_vh(8.0.px(), 10.0.px()))
            .corner_radius(15.0.px())
            .background_color(xilem::Color::TRANSPARENT)
            .border_width(0.0.px());
            let choice = if let Some(targets) = targets {
                Record {
                    inner: choice,
                    target: targets[index].clone(),
                }
                .boxed()
            } else {
                choice.boxed()
            };
            crate::ui::canvas_search::material::choice(choice, checked, palette).boxed()
        })
        .collect();
    radio_group(
        flex_col(rows)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(0.0.px()),
    )
}
