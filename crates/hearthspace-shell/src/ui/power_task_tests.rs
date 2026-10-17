use super::*;
use crate::{
    battery::{BatteryKind, BatteryState, BatteryStatus},
    ui::{
        session_controls,
        test_support::{Ui, ids},
    },
};
use masonry::accesskit::Role;
use std::{
    io::{Read, Write},
    os::unix::net::UnixListener,
    thread,
};
use xilem::{WidgetView, view::sized_box};

fn power(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(session_controls::power_panel(
        &state.outputs[0],
        &state.power,
        state.battery.as_ref(),
    ))
}

fn bar(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(crate::ui::corners::system_view(state, &state.outputs[0]))
}

fn battery(state: BatteryState) -> BatteryStatus {
    BatteryStatus {
        percentage: 57,
        state,
        kind: BatteryKind::Battery,
        on_battery: state == BatteryState::Discharging,
        warning: None,
    }
}

#[test]
fn suspend_request_uses_private_endpoint_and_preserves_rejection() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("power-control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let worker = thread::spawn(move || {
        for command in ["status", "suspend"] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut received = String::new();
            stream.read_to_string(&mut received).unwrap();
            assert_eq!(received, format!("{{\"command\":\"{command}\"}}\n"));
            let reply = if command == "status" {
                r#"{"ok":true,"available":true,"busy":false,"error":null}"#
            } else {
                r#"{"ok":false,"available":false,"busy":false,"error":"Another application is blocking suspend."}"#
            };
            writeln!(stream, "{reply}").unwrap();
        }
    });
    assert!(request(&socket, "status").unwrap().available);
    assert_eq!(
        request(&socket, "suspend").unwrap_err(),
        "Another application is blocking suspend."
    );
    worker.join().unwrap();
}

#[test]
fn missing_native_service_is_unavailable_and_cannot_queue_suspend() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    suspend(&mut state, "eDP-1");
    assert!(!state.power.pending);
    let ui = Ui::new(state, power, (300, 280));
    assert!(
        ui.harness
            .access_node(ui.button("Suspend"))
            .unwrap()
            .is_disabled()
    );
    assert!(!ui.state.power.can_suspend());
}

#[test]
fn batteryless_desktop_can_suspend_and_repeated_clicks_queue_only_once() {
    let mut state = ShellState::new("/unused".into(), vec!["DP-1".into()]);
    state.toggle_power("DP-1");
    let (sender, mut commands) = tokio::sync::mpsc::channel(1);
    state.power.sender = Some(sender);
    state.power.status.available = true;
    assert!(state.battery.is_none());
    let mut ui = Ui::new(state, power, (300, 280));
    assert!(
        !ui.harness
            .access_node(ui.button("Suspend"))
            .unwrap()
            .is_disabled()
    );
    ui.harness.mouse_click_on(ui.button("Suspend"), None);
    ui.dispatch(&power);
    assert_eq!(commands.try_recv().unwrap(), "DP-1");
    assert!(ui.state.power.pending);
    assert!(
        ui.harness
            .access_node(ui.button("Suspending…"))
            .unwrap()
            .is_disabled()
    );
    ui.activate(ui.button("Suspending…"));
    ui.dispatch(&power);
    suspend(&mut ui.state, "DP-1");
    assert!(commands.try_recv().is_err());
}

#[test]
fn battery_updates_are_accessible_and_removal_removes_indicator() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.battery = Some(battery(BatteryState::Charging));
    let mut ui = Ui::new(state, bar, (320, 48));
    let labels = |ui: &Ui<_>| {
        ids(&ui.harness)
            .into_iter()
            .filter_map(|id| {
                ui.harness
                    .access_node(id)
                    .and_then(|node| node.label().or_else(|| node.value()))
            })
            .collect::<Vec<_>>()
    };
    assert!(labels(&ui).contains(&"57%".into()));
    assert!(labels(&ui).contains(&format!(
        "Battery: {}",
        ui.state.battery.as_ref().unwrap().state_label()
    )));
    let before = ui.harness.render();
    ui.state.battery = Some(battery(BatteryState::Discharging));
    ui.rebuild(&bar);
    ui.dispatch(&bar);
    assert_ne!(
        before,
        ui.harness.render(),
        "charge icon must visibly change"
    );
    let expected = format!(
        "Battery: {}",
        ui.state.battery.as_ref().unwrap().state_label()
    );
    assert!(labels(&ui).contains(&expected));
    for name in ["Control center", "Power"] {
        let bounds = ui
            .harness
            .access_node(ui.button(name))
            .unwrap()
            .bounding_box()
            .unwrap();
        assert!(bounds.x0 >= 0.0 && bounds.x1 <= 320.0, "{name}: {bounds:?}");
    }
    ui.state.battery = None;
    ui.rebuild(&bar);
    ui.dispatch(&bar);
    assert!(!labels(&ui).contains(&"57%".into()));
    assert!(
        !ids(&ui.harness)
            .into_iter()
            .any(|id| ui
                .harness
                .access_node(id)
                .is_some_and(|node| node.role() == Role::Image
                    && node
                        .label()
                        .is_some_and(|label| label.starts_with("Battery:"))))
    );
}

#[test]
fn power_menu_describes_battery_state_and_capability_changes() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.battery = Some(battery(BatteryState::Charging));
    let expected = state.battery.as_ref().unwrap().accessible_label();
    let mut ui = Ui::new(state, power, (300, 300));
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness
            .access_node(id)
            .is_some_and(|node| node.label().or_else(|| node.value()).as_deref() == Some(&expected))
    }));
    let (sender, _receiver) = tokio::sync::mpsc::channel(1);
    ui.state.power.sender = Some(sender);
    ui.state.power.status.available = true;
    ui.rebuild(&power);
    ui.dispatch(&power);
    assert!(
        !ui.harness
            .access_node(ui.button("Suspend"))
            .unwrap()
            .is_disabled()
    );
    ui.state.power.status.available = false;
    ui.rebuild(&power);
    ui.dispatch(&power);
    assert!(
        ui.harness
            .access_node(ui.button("Suspend"))
            .unwrap()
            .is_disabled()
    );
}
