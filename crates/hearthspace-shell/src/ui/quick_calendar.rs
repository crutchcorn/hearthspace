use super::ShellState;
use chrono::{Datelike, Local, NaiveDate};
use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, FlexExt, flex_col, flex_row, label, sized_box},
};

pub(super) fn view() -> impl WidgetView<ShellState> + use<> {
    let today = Local::now().date_naive();
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
                .color(components::TEXT_MUTED)
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
                            label(day.map_or(String::new(), |day| day.to_string()))
                                .text_size(12.0)
                                .text_alignment(xilem::TextAlign::Center)
                                .color(if selected {
                                    components::ACCENT_STRONG
                                } else {
                                    components::TEXT
                                }),
                        )
                        .height(34.0.px())
                        .corner_radius(10.0.px())
                        .background_color(if selected {
                            components::ACCENT_SUBTLE
                        } else {
                            xilem::Color::TRANSPARENT
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
