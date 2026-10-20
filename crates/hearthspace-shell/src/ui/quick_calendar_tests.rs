use super::*;
use crate::ui::test_support::{Ui, ids};
use hearthspace_ui_components as components;
use masonry::{accesskit::Role, kurbo::Rect, properties::Background};

fn assert_dates_centered(today: NaiveDate, width: u32) {
    let state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    let ui = Ui::new(state, |_| sized_box(view_for_date(today)), (width, 400));
    let highlights: Vec<_> = ids(&ui.harness)
        .into_iter()
        .filter_map(|id| {
            let widget = ui.harness.get_widget_with_id(id);
            (*widget.get_prop::<Background>() == Background::Color(components::ACCENT_SUBTLE)).then(
                || {
                    widget
                        .ctx()
                        .window_transform()
                        .transform_rect_bbox(widget.ctx().border_box())
                },
            )
        })
        .collect();
    assert_eq!(highlights.len(), 1, "only today's cell is highlighted");
    let highlight = highlights[0];
    assert_eq!(highlight.height(), 34.0);
    let selected_center = highlight.center();
    let week_start = today.day() - today.weekday().num_days_from_sunday();

    // Text-run bounds describe the laid-out text, unlike the label's bounds,
    // which may expand to the whole cell even when its text stays at the top.
    for day in week_start..week_start + 7 {
        let day_text = day.to_string();
        let label = ids(&ui.harness)
            .into_iter()
            .find_map(|id| {
                ui.harness.access_node(id).filter(|node| {
                    node.role() == Role::Label && node.value().as_deref() == Some(day_text.as_str())
                })
            })
            .unwrap_or_else(|| panic!("missing calendar date {day}"));
        let text = label
            .children()
            .filter(|node| node.role() == Role::TextRun)
            .filter_map(|node| node.bounding_box())
            .map(|rect| Rect::new(rect.x0, rect.y0, rect.x1, rect.y1))
            .reduce(|left, right| left.union(right))
            .expect("a date has laid-out text");
        assert!(
            (text.center().y - selected_center.y).abs() <= 1.0,
            "date {day} must be vertically centered with today's highlight: \
             text {text:?}, highlight {highlight:?}"
        );
        assert!(text.y0 >= highlight.y0 && text.y1 <= highlight.y1);
        if day == today.day() {
            assert!(
                (text.center().x - selected_center.x).abs() <= 1.0,
                "today's text must be horizontally centered in its highlight"
            );
        }
    }
}

#[test]
fn single_digit_dates_and_adjacent_double_digits_center_in_their_cells() {
    assert_dates_centered(NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(), 272);
}

#[test]
fn double_digit_dates_center_in_the_active_background() {
    assert_dates_centered(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap(), 320);
}
