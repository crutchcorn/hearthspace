use hearthspace_ui_components as components;
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

pub(super) fn button(
    output: &ShellOutputInstance,
    items: &[Notification],
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    let count = items.iter().filter(|item| item.active).count();
    flex_row((
        icon_button(
            Icon::Bell {
                active: items.iter().any(|item| item.active),
            },
            "Notifications",
            output.show_notifications,
            move |state| state.toggle_notifications(&key),
        ),
        label(if count == 0 {
            "All clear".into()
        } else {
            format!("{count} new")
        })
        .text_size(12.0)
        .color(if count > 0 {
            components::ACCENT_STRONG
        } else {
            components::TEXT_MUTED
        }),
    ))
    .gap(2.0.px())
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
                        .color(components::TEXT_MUTED),
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
        .background_color(components::SURFACE_CONTROL),
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
        .color(components::TEXT_MUTED)
        .boxed(),
        label(item.summary)
            .text_size(15.0)
            .color(components::TEXT)
            .line_break_mode(LineBreaking::WordWrap)
            .boxed(),
    ];
    if !item.body.is_empty() {
        rows.push(
            label(item.body)
                .font(GenericFamily::SystemUi)
                .text_size(12.0)
                .color(components::TEXT_MUTED)
                .line_break_mode(LineBreaking::WordWrap)
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
                components::icons::icon(components::icons::Icon::Bell, 16.0).color(if active {
                    components::ACCENT_STRONG
                } else {
                    components::TEXT_MUTED
                }),
            )
            .width(29.0.px())
            .height(29.0.px())
            .corner_radius(10.0.px())
            .background_color(if active {
                components::ACCENT_SUBTLE
            } else {
                components::SURFACE_CONTROL
            }),
            flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(7.0.px())
                .flex(1.0),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(14.0.px()),
    )
    .background_color(if item.urgency == 2 && item.active {
        components::DANGER_SUBTLE
    } else {
        xilem::Color::TRANSPARENT
    })
    .padding(20.0.px())
}
