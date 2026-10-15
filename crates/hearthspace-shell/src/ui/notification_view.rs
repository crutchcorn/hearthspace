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
    state::{ShellOutputInstance, ShellState},
};

pub(super) fn button(
    output: &ShellOutputInstance,
    items: &[Notification],
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    icon_button(
        Icon::Bell {
            active: items.iter().any(|item| item.active),
        },
        "Notifications",
        output.show_notifications,
        move |state| state.toggle_notifications(&key),
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
        rows.push(components::field_label(error).boxed());
    }
    if items.is_empty() {
        rows.push(components::muted_text("No notifications yet").boxed());
    }
    rows.extend(items.into_iter().map(|item| notification_row(item).boxed()));
    sized_box(
        flex_col((
            flex_row((
                components::section_title("Notifications"),
                FlexSpacer::Flex(1.0),
                close,
            ))
            .gap(4.0.px()),
            components::scroll_area(
                flex_col(rows)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .gap(8.0.px()),
            )
            .flex(1.0),
        ))
        .gap(8.0.px()),
    )
    .background_color(components::SURFACE)
    .padding(12.0.px())
    .border(components::BORDER, 1.0.px())
    .corner_radius(10.0.px())
}

fn notification_row(item: Notification) -> impl WidgetView<ShellState> + use<> {
    let mut rows: Vec<Box<AnyWidgetView<ShellState>>> = vec![
        components::muted_text(if item.active {
            item.app_name
        } else {
            format!("History · {}", item.app_name)
        })
        .boxed(),
        components::field_label(item.summary)
            .line_break_mode(LineBreaking::WordWrap)
            .boxed(),
    ];
    if !item.body.is_empty() {
        rows.push(
            label(item.body)
                .font(GenericFamily::SystemUi)
                .text_size(13.0)
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
    sized_box(
        flex_col(rows)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(4.0.px()),
    )
    .background_color(if item.urgency == 2 && item.active {
        components::DANGER_SUBTLE
    } else {
        components::SURFACE_SUBTLE
    })
    .padding(8.0.px())
}
