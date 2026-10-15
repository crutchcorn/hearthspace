use masonry::accesskit::Role;
use xilem::{WidgetView, view::sized_box};

use super::*;
use crate::ui::{
    session_controls::power_panel,
    test_support::{Ui, ids},
};

fn bar(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(bar_view(
        &state.outputs[0],
        state.clock.clone(),
        &state.notifications,
    ))
}

fn power(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(power_panel(&state.outputs[0]))
}

fn search(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(launcher::launcher_view(
        &state.outputs[0],
        launcher::results(state, &state.outputs[0]),
        None,
    ))
}

#[test]
fn bar_icons_are_named_visible_on_small_outputs_and_toggle_exclusive_panels() {
    for width in [320, 800] {
        let state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
        let mut ui = Ui::new(state, bar, (width, 48));
        let buttons = ids(&ui.harness)
            .into_iter()
            .filter_map(|id| {
                let node = ui.harness.access_node(id)?;
                (node.role() == Role::Button).then(|| node.label().unwrap())
            })
            .collect::<Vec<_>>();
        assert_eq!(buttons.len(), 4, "only the four icon buttons should remain");
        for removed in [
            "Overview",
            "Settings",
            "Windows",
            "Reset view",
            "Lock",
            "Log out",
        ] {
            assert!(!buttons.iter().any(|name| name == removed));
        }
        for name in ["Search", "Notifications", "Canvas map", "Power"] {
            let id = ui.button(name);
            let bounds = ui.harness.access_node(id).unwrap().bounding_box().unwrap();
            assert!(
                bounds.x0 >= 0.0 && bounds.x1 <= f64::from(width),
                "{name} at width {width}: {bounds:?}"
            );
            // Real pointer activation catches clipping that accessibility bypasses.
            ui.harness.mouse_click_on(id, None);
            ui.dispatch(&bar);
            let output = &ui.state.outputs[0];
            assert_eq!(
                [
                    output.show_search,
                    output.show_notifications,
                    output.show_map,
                    output.show_power
                ]
                .into_iter()
                .filter(|open| *open)
                .count(),
                1
            );
            assert!(match name {
                "Search" => output.show_search,
                "Notifications" => output.show_notifications,
                "Canvas map" => output.show_map,
                "Power" => output.show_power,
                _ => unreachable!(),
            });
        }
        ui.harness.mouse_click_on(ui.button("Power"), None);
        ui.dispatch(&bar);
        assert!(!ui.state.outputs[0].show_power);
    }
}

#[test]
fn power_dropdown_keeps_confirmation_inside_and_cancel_does_not_end_session() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.toggle_power("eDP-1");
    let mut ui = Ui::new(state, power, (300, 230));
    ui.button("Lock");
    ui.activate(ui.button("Log out"));
    ui.dispatch(&power);
    assert!(ui.state.outputs[0].confirm_logout);
    assert!(ui.state.outputs[0].show_power);
    ui.button("Confirm log out");
    ui.activate(ui.button("Cancel"));
    ui.dispatch(&power);
    assert!(!ui.state.outputs[0].confirm_logout);
    assert!(ui.state.outputs[0].show_power);
    assert!(ui.state.outputs[0].launch_error.is_none());
    ui.button("Lock");
    ui.activate(ui.button("Close power menu"));
    ui.dispatch(&power);
    assert!(!ui.state.outputs[0].show_power);
}

#[test]
fn empty_search_opens_with_focus_and_escape_closes_without_a_command() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.toggle_search("eDP-1");
    let mut ui = Ui::new(state, search, (360, 144));
    assert!(ui.harness.focused_widget_id().is_some());
    ui.harness.focus_on(Some(ui.button("Close search")));
    ui.harness
        .process_text_event(masonry::core::TextEvent::key_down(
            masonry::core::keyboard::Key::Named(masonry::core::keyboard::NamedKey::Escape),
        ));
    ui.dispatch(&search);
    assert!(!ui.state.outputs[0].show_search);
    assert!(ui.state.outputs[0].query.is_empty());
    assert!(ui.state.outputs[0].launch_error.is_none());
}

#[test]
fn search_header_and_field_stay_in_bounds_when_results_scroll() {
    for width in [280, 360] {
        let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
        state.catalog = Default::default();
        state.change_query("eDP-1", "Notes".into());
        state.windows = (1..=4)
            .map(|id| crate::ui::window_catalog::RunningWindow {
                id,
                title: format!("Notes {id}"),
                app_id: "editor".into(),
                focused: false,
            })
            .collect();
        let mut ui = Ui::new(state, search, (width, 176));
        let input = ids(&ui.harness)
            .into_iter()
            .find(|id| {
                ui.harness
                    .get_widget_with_id(*id)
                    .downcast::<masonry::widgets::TextInput>()
                    .is_some()
            })
            .unwrap();
        let bounds = |ui: &Ui<_>| {
            let widget = ui.harness.get_widget_with_id(input);
            widget
                .ctx()
                .window_transform()
                .transform_rect_bbox(widget.ctx().border_box())
        };
        let input_before = bounds(&ui);
        let close = ui.button("Close search");
        let close_before = ui
            .harness
            .access_node(close)
            .unwrap()
            .bounding_box()
            .unwrap();
        assert!(
            input_before.x0 >= 12.0 && input_before.x1 <= f64::from(width) - 12.0,
            "{input_before:?}"
        );
        assert!(close_before.x1 <= f64::from(width) - 12.0);
        ui.harness.scroll_into_view(ui.button("Window: Notes 4"));
        assert_eq!(bounds(&ui), input_before);
        assert_eq!(
            ui.harness
                .access_node(close)
                .unwrap()
                .bounding_box()
                .unwrap(),
            close_before
        );
        ui.harness.mouse_click_on(close, None);
        ui.dispatch(&search);
        assert!(!ui.state.outputs[0].show_search);
    }
}
