use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{layout::AsUnit, parley::fontique::GenericFamily, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, FlexExt, flex_col, flex_row, label, sized_box},
};

use super::{
    command_client::send_command,
    notifications::{self, Notification},
    state::{ShellOutputInstance, ShellState},
};

pub(super) fn button_label(items: &[Notification]) -> String {
    let active = items.iter().filter(|item| item.active).collect::<Vec<_>>();
    let Some(newest) = active.first() else {
        return "Notifications".into();
    };
    let single_line = newest
        .summary
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut summary = single_line.chars().take(10).collect::<String>();
    if single_line.chars().count() > 10 {
        summary.push('…');
    }
    format!("Notifications ({}) · {summary}", active.len())
}

pub(super) fn button(
    output: &ShellOutputInstance,
    items: &[Notification],
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    components::compact_action_button(button_label(items), move |state: &mut ShellState| {
        if let Some(output) = state.outputs.iter_mut().find(|output| output.key() == key) {
            output.show_notifications = !output.show_notifications;
            output.show_windows = false;
            output.query.clear();
            output.confirm_logout = false;
        }
        let _ = send_command(&state.command_socket, "focus-launcher");
    })
}

pub(super) fn panel(
    output: &ShellOutputInstance,
    items: Vec<Notification>,
    error: Option<String>,
) -> impl WidgetView<ShellState> + use<> {
    let key = output.key().to_string();
    let close =
        components::compact_action_button("Close notifications", move |state: &mut ShellState| {
            state.dismiss_launcher(&key);
            let _ = send_command(&state.command_socket, "focus-launcher");
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
            flex_row((components::section_title("Notifications"), close)).gap(4.0.px()),
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
    .padding(8.0.px())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_counts_only_active_notifications_and_truncates_unicode_safely() {
        let item = Notification {
            id: 1,
            app_name: "Chat".into(),
            summary: "🌍".repeat(25),
            body: String::new(),
            actions: vec![],
            urgency: 1,
            active: true,
        };
        let mut closed = item.clone();
        closed.id = 2;
        closed.active = false;
        let label = button_label(&[closed, item]);
        assert_eq!(label, format!("Notifications (1) · {}…", "🌍".repeat(10)));
        assert_eq!(button_label(&[]), "Notifications");
    }
}
