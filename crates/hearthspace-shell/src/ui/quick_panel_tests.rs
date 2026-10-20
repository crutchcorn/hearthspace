use super::*;
use crate::ui::test_support::{Ui, ids};

fn view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(panel(state, &state.outputs[0]))
}

#[test]
fn status_dropdowns_replace_each_other_and_dismiss_with_the_existing_panel_lifecycle() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into(), "DP-2".into()]);
    state.toggle_search("DP-2");
    state.toggle_quick_panel("eDP-1", Kind::Battery);
    assert!(!state.outputs[1].show_search);
    assert!(state.outputs[0].show_controls);
    state.toggle_quick_panel("eDP-1", Kind::Sound);
    assert!(state.outputs[0].show_controls);
    assert_eq!(state.outputs[0].quick_panel, Kind::Sound);
    state.toggle_quick_panel("eDP-1", Kind::Brightness);
    assert!(state.outputs[0].show_controls);
    assert_eq!(state.outputs[0].quick_panel, Kind::Brightness);
    state.toggle_quick_panel("eDP-1", Kind::Brightness);
    assert!(!state.outputs[0].show_controls);
    state.toggle_quick_panel("eDP-1", Kind::Clock);
    state.dismiss_panels();
    assert!(!state.outputs[0].show_controls);
}

#[test]
fn wifi_and_missing_battery_never_present_simulated_readings() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.toggle_quick_panel("eDP-1", Kind::Wifi);
    let mut ui = Ui::new(state, view, (320, 300));
    let group = |ui: &Ui<_>, name: &str| {
        ids(&ui.harness).into_iter().any(|id| {
            ui.harness.access_node(id).is_some_and(|node| {
                node.role() == masonry::accesskit::Role::Group
                    && node.label().as_deref() == Some(name)
            })
        })
    };
    let text = |ui: &Ui<_>| {
        ids(&ui.harness)
            .into_iter()
            .filter_map(|id| {
                ui.harness.access_node(id).and_then(|node| {
                    node.label()
                        .or_else(|| node.value())
                        .map(|value| value.to_string())
                })
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert!(text(&ui).contains("network tray"));
    assert!(!text(&ui).contains("Studio network"));
    assert!(group(&ui, "Wi-Fi"));
    ui.state.toggle_quick_panel("eDP-1", Kind::Battery);
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(text(&ui).contains("No battery detected"));
    assert!(!text(&ui).contains("0%"));
    assert!(group(&ui, "Battery"));
    assert!(!group(&ui, "Wi-Fi"));
}

#[test]
fn changing_status_panels_does_not_retain_the_previous_content_height() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.toggle_quick_panel("eDP-1", Kind::Clock);
    let mut ui = Ui::new(state, view, (320, 400));
    ui.state.toggle_quick_panel("eDP-1", Kind::Wifi);
    ui.rebuild(&view);
    ui.dispatch(&view);
    let switched = popup::size(&ui.state.outputs[0], PopupKind::Controls, 320.0, 200.0);

    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.toggle_quick_panel("eDP-1", Kind::Wifi);
    let fresh = Ui::new(state, view, (320, 400));
    assert_eq!(
        switched,
        popup::size(&fresh.state.outputs[0], PopupKind::Controls, 320.0, 200.0)
    );
}

#[test]
fn battery_dropdown_shows_one_reading_and_state_with_only_real_charge_warnings() {
    use crate::battery::{BatteryKind, BatteryState, BatteryStatus, BatteryWarning};
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.toggle_quick_panel("eDP-1", Kind::Battery);
    state.battery = Some(BatteryStatus {
        percentage: 57,
        state: BatteryState::Charging,
        kind: BatteryKind::Battery,
        on_battery: false,
        warning: None,
    });
    let mut ui = Ui::new(state, view, (320, 240));
    let text = |ui: &Ui<_>| {
        ids(&ui.harness)
            .into_iter()
            .filter_map(|id| {
                ui.harness
                    .access_node(id)
                    .and_then(|node| node.label().or_else(|| node.value()))
            })
            .collect::<Vec<_>>()
    };
    let labels = text(&ui);
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.as_str() == "57%")
            .count(),
        1
    );
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.as_str() == "Charging")
            .count(),
        1
    );
    assert!(
        !labels
            .iter()
            .any(|label| label.contains("Battery: 57%") || label.contains("Connect to power"))
    );

    let battery = ui.state.battery.as_mut().unwrap();
    battery.percentage = 3;
    battery.state = BatteryState::Discharging;
    battery.on_battery = true;
    battery.warning = Some(BatteryWarning::Critical);
    ui.rebuild(&view);
    ui.dispatch(&view);
    let labels = text(&ui);
    assert!(labels.contains(&"3%".into()));
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.as_str() == "Discharging")
            .count(),
        1
    );
    assert!(labels.contains(&"Critical charge. Connect to power and save your work.".into()));

    ui.state.battery.as_mut().unwrap().warning = None;
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(
        !text(&ui)
            .iter()
            .any(|label| label.contains("Connect to power"))
    );
}
