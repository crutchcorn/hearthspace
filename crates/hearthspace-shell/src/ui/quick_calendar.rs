use super::ShellState;
use crate::ui::appearance::{ThemeStyle as _, Tone};
use chrono::{Datelike, Local, NaiveDate};
use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, FlexExt, MainAxisAlignment, flex_col, flex_row, label, sized_box},
};

pub(in crate::ui) fn view() -> impl WidgetView<ShellState> + use<> {
    view_for_date(Local::now().date_naive())
}

fn view_for_date(today: NaiveDate) -> impl WidgetView<ShellState> + use<> {
    let first = today.with_day(1).unwrap();
    let next = if today.month() == 12 {
        NaiveDate::from_ymd_opt(today.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(today.year(), today.month() + 1, 1)
    }
    .unwrap();
    let days = (next - first).num_days() as usize;
    let offset = first.weekday().num_days_from_sunday() as usize;
    let mut rows: Vec<Box<AnyWidgetView<ShellState>>> = vec![
        label(today.format("%B %Y").to_string())
            .text_size(20.0)
            .boxed(),
        components::muted_text(today.format("%A, %B %-d").to_string()).boxed(),
        flex_row(["S", "M", "T", "W", "T", "F", "S"].map(|day| {
            label(day)
                .text_size(11.0)
                .text_alignment(xilem::TextAlign::Center)
                .theme_color(Tone::Muted)
                .flex(1.0)
        }))
        .boxed(),
    ];
    for week in 0..(days + offset).div_ceil(7) {
        rows.push(
            flex_row(
                (0..7)
                    .map(|weekday| {
                        let cell = week * 7 + weekday;
                        let day = cell
                            .checked_sub(offset)
                            .map(|day| day + 1)
                            .filter(|day| *day <= days);
                        let selected = day == Some(today.day() as usize);
                        sized_box(
                            // Keep the text at its natural line height; a label
                            // stretched to the cell height paints at the top.
                            flex_col((label(day.map_or(String::new(), |day| day.to_string()))
                                .text_size(12.0)
                                .text_alignment(xilem::TextAlign::Center)
                                .theme_color(if selected { Tone::Strong } else { Tone::Text }),))
                            .main_axis_alignment(MainAxisAlignment::Center)
                            .cross_axis_alignment(CrossAxisAlignment::Stretch),
                        )
                        .height(34.0.px())
                        .corner_radius(10.0.px())
                        .theme_background(if selected {
                            Tone::AccentSoft
                        } else {
                            Tone::Transparent
                        })
                        .flex(1.0)
                    })
                    .collect::<Vec<_>>(),
            )
            .gap(2.0.px())
            .boxed(),
        );
    }
    flex_col(rows)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(8.0.px())
}

#[cfg(test)]
#[path = "quick_calendar_tests.rs"]
mod tests;
