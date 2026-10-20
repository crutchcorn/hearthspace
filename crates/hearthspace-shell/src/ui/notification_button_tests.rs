use super::*;
use crate::ui::test_support::{Ui, ids};
use masonry::{accesskit::Role, core::WidgetId, properties::BorderColor};

fn view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(super::super::corners::notifications_view(
        state,
        &state.outputs[0],
    ))
}

fn state() -> ShellState {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.outputs[0].corner_widths[0] = f64::from(hearthspace_common::NOTIFICATION_CORNER_WIDTH);
    state
}

fn status(ui: &Ui<impl WidgetView<ShellState, Widget: Sized>>, text: &str) -> WidgetId {
    ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.role() == Role::Label && node.value().as_deref() == Some(text)
            })
        })
        .unwrap_or_else(|| panic!("Missing notification status: {text}"))
}

#[test]
fn icon_status_and_right_padding_share_one_notification_button_and_focus_outline() {
    let width = hearthspace_common::NOTIFICATION_CORNER_WIDTH as u32;
    let mut ui = Ui::new(state(), view, (width, 54));
    let button = ui.button("Notifications");
    assert_eq!(
        ids(&ui.harness)
            .into_iter()
            .filter(|id| ui.harness.access_node(*id).unwrap().role() == Role::Button)
            .count(),
        1
    );
    let bounds = ui
        .harness
        .access_node(button)
        .unwrap()
        .bounding_box()
        .unwrap();
    let status = ui.harness.access_node(status(&ui, "All clear")).unwrap();
    let text = status.bounding_box().unwrap();
    let bell = ids(&ui.harness)
        .into_iter()
        .find_map(|id| {
            ui.harness.access_node(id).filter(|node| {
                node.role() == Role::Image && node.label().as_deref() == Some("Notifications")
            })
        })
        .unwrap()
        .bounding_box()
        .unwrap();
    assert_eq!(
        bell.x0 - bounds.x0,
        bounds.x1 - text.x1,
        "the bell and status must have equal padding at the button's ends"
    );
    assert!(bounds.x0 < text.x0 && text.x1 + 10.0 <= bounds.x1);
    assert!(bounds.y0 <= text.y0 && text.y1 <= bounds.y1);

    ui.harness
        .mouse_move(((text.x0 + text.x1) / 2.0, (text.y0 + text.y1) / 2.0));
    ui.harness.mouse_button_press(None);
    ui.harness.mouse_button_release(None);
    ui.dispatch(&view);
    assert!(
        ui.state.outputs[0].show_notifications,
        "the status text is clickable"
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(button));
    assert_eq!(
        ui.harness
            .get_widget_with_id(button)
            .get_prop::<BorderColor>()
            .color,
        components::TEXT
    );
    assert_eq!(
        ui.harness.access_node(button).unwrap().bounding_box(),
        Some(bounds)
    );

    ui.harness
        .mouse_move((bounds.x1 - 5.0, (bounds.y0 + bounds.y1) / 2.0));
    ui.harness.mouse_button_press(None);
    ui.harness.mouse_button_release(None);
    ui.dispatch(&view);
    assert!(
        !ui.state.outputs[0].show_notifications,
        "the trailing padding is clickable"
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(button));
}

#[test]
fn unread_counts_keep_the_same_named_button_and_fit_inside_its_padding() {
    let width = hearthspace_common::NOTIFICATION_CORNER_WIDTH as u32;
    let mut ui = Ui::new(state(), view, (width, 54));
    let button = ui.button("Notifications");
    let bounds = ui
        .harness
        .access_node(button)
        .unwrap()
        .bounding_box()
        .unwrap();
    for (count, text) in [(0, "All clear"), (1, "1 new"), (24, "24 new")] {
        ui.state.notifications = (1..=count)
            .map(|id| Notification {
                id,
                app_name: "Fixture".into(),
                summary: "New notification".into(),
                body: String::new(),
                actions: vec![],
                urgency: 1,
                active: true,
            })
            .collect();
        ui.rebuild(&view);
        ui.dispatch(&view);
        assert_eq!(ui.button("Notifications"), button);
        assert_eq!(
            ui.harness.access_node(button).unwrap().bounding_box(),
            Some(bounds)
        );
        let status = ui.harness.access_node(status(&ui, text)).unwrap();
        for run in status.children() {
            let text = run.bounding_box().unwrap();
            assert!(
                text.x0 >= bounds.x0 && text.x1 + 10.0 <= bounds.x1,
                "{text:?}"
            );
        }
    }
}

#[test]
fn notification_button_stays_inside_a_narrow_corner() {
    let ui = Ui::new(state(), view, (80, 54));
    let button = ui.harness.access_node(ui.button("Notifications")).unwrap();
    let bounds = button.bounding_box().unwrap();
    assert!(bounds.x0 >= 0.0 && bounds.x1 <= 80.0, "{bounds:?}");
    assert_eq!(bounds.height(), 44.0);
}
