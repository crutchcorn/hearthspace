use crate::ui::appearance::{ThemeStyle as _, Tone};
use hearthspace_ui_components as components;
#[cfg(test)]
use xilem::masonry::properties::{Dimensions, Padding};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{layout::AsUnit, parley::fontique::GenericFamily, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, FlexExt, FlexSpacer, flex_col, flex_row, label, sized_box},
};

use super::{
    icons::{Icon, icon_button},
    notifications::{self, Notification},
    popup::{self, PopupKind},
    state::{ShellOutputInstance, ShellState},
};

#[cfg(test)]
pub(super) fn button(
    output: &ShellOutputInstance,
    items: &[Notification],
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    let count = items.iter().filter(|item| item.active).count();
    // The corner contributes 4px padding and a 1px border on each side.
    let width = (output.corner_widths[0] - 10.0).clamp(
        0.0,
        f64::from(hearthspace_common::NOTIFICATION_CORNER_WIDTH) - 10.0,
    );
    let padding_budget = (width - 20.0).max(0.0);
    let padding = 10.0_f64.min(padding_budget / 2.0);
    let content_width = (width - 2.0 * padding - 2.0).max(0.0);
    let icon_size = content_width.min(18.0);
    let remaining = (content_width - icon_size).max(0.0);
    let gap = remaining.min(9.0);
    let color = if count > 0 || output.show_notifications {
        Tone::Strong
    } else {
        Tone::Muted
    };
    components::button_surface(
        xilem::view::button(
            flex_row((
                components::icons::icon(
                    if count > 0 {
                        components::icons::Icon::BellUnread
                    } else {
                        components::icons::Icon::Bell
                    },
                    icon_size,
                )
                .label("Notifications")
                .theme_color(color),
                // A named group exposes the changing status separately from
                // the button's stable name, supplied by the bell above.
                super::popup_group::group(
                    "Notification status",
                    label(if count == 0 {
                        "All clear".into()
                    } else {
                        format!("{count} new")
                    })
                    .text_size(13.0)
                    .width((remaining - gap).px())
                    .line_break_mode(LineBreaking::Clip)
                    .theme_color(color),
                ),
            ))
            .gap(gap.px()),
            move |state| state.toggle_notifications(&key),
        )
        .dims(Dimensions::fixed(width.px(), 44.0.px()))
        .padding(Padding {
            left: padding.px(),
            right: padding.px(),
            ..Padding::ZERO
        })
        .corner_radius(components::RADIUS_CONTROL.px()),
        components::ButtonSurface::Bare,
        output.show_notifications,
    )
}

pub(super) fn panel(
    output: &ShellOutputInstance,
    items: Vec<Notification>,
    error: Option<String>,
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    let close = icon_button(Icon::Close, "Close notifications", false, move |state| {
        state.dismiss_notifications(&key);
    });
    let mut rows: Vec<Box<AnyWidgetView<ShellState>>> = Vec::new();
    if let Some(error) = error {
        rows.push(
            components::field_label(error)
                .line_break_mode(LineBreaking::WordWrap)
                .boxed(),
        );
    }
    if items.is_empty() {
        rows.push(
            sized_box(
                flex_col((
                    components::icons::icon(components::icons::Icon::Bell, 28.0)
                        .theme_color(Tone::Muted),
                    label("You're all caught up").text_size(20.0),
                    components::muted_text("No notifications yet"),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(12.0.px()),
            )
            .padding(24.0.px())
            .height(200.0.px())
            .boxed(),
        );
    }
    rows.extend(items.into_iter().map(|item| notification_row(item).boxed()));
    popup::panel_with_header(
        output,
        PopupKind::Notifications,
        sized_box(
            flex_row((
                label("Notifications").text_size(24.0),
                FlexSpacer::Flex(1.0),
                close,
            ))
            .gap(4.0.px()),
        )
        .padding(14.0.px())
        .theme_background(Tone::Control),
        flex_col(rows)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(0.0.px()),
    )
}

fn notification_row(item: Notification) -> impl WidgetView<ShellState> + use<> {
    let mut rows: Vec<Box<AnyWidgetView<ShellState>>> = vec![
        label(if item.active {
            item.app_name
        } else {
            format!("History · {}", item.app_name)
        })
        .text_size(11.0)
        .theme_color(Tone::Muted)
        .boxed(),
        label(item.summary)
            .text_size(15.0)
            .line_break_mode(LineBreaking::WordWrap)
            .boxed(),
    ];
    if !item.body.is_empty() {
        rows.push(
            label(item.body)
                .font(GenericFamily::SystemUi)
                .text_size(12.0)
                .line_break_mode(LineBreaking::WordWrap)
                .theme_color(Tone::Muted)
                .boxed(),
        );
    }
    if item.active {
        for action in item.actions {
            let id = item.id;
            rows.push(
                components::compact_action_button(action.label, move |state: &mut ShellState| {
                    notifications::invoke(state, id, &action.key);
                })
                .boxed(),
            );
        }
        rows.push(
            components::compact_action_button("Dismiss", move |state: &mut ShellState| {
                notifications::dismiss(state, item.id);
            })
            .boxed(),
        );
    }
    let active = item.active;
    sized_box(
        flex_row((
            sized_box(
                components::icons::icon(components::icons::Icon::Bell, 16.0)
                    .theme_color(if active { Tone::Strong } else { Tone::Muted }),
            )
            .width(29.0.px())
            .height(29.0.px())
            .corner_radius(10.0.px())
            .theme_background(if active {
                Tone::AccentSoft
            } else {
                Tone::Control
            }),
            flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(7.0.px())
                .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(14.0.px()),
    )
    .theme_background(if item.urgency == 2 && item.active {
        Tone::Danger
    } else {
        Tone::Transparent
    })
    .padding(20.0.px())
}

#[cfg(test)]
#[path = "notification_button_tests.rs"]
mod button_tests;
