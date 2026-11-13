//! Bind the shared radio controls to this app's asynchronous service worker.
use super::state::ShellState;
use hearthspace_desktop::wireless::{
    self, Command,
    state::{State, Update},
};
use hearthspace_ui_components::{
    self as components,
    radio_panel::{Panel, Row},
};
pub(crate) use wireless::state::State as RadioState;
use xilem::{
    ViewCtx, WidgetView,
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};
pub(crate) fn task() -> impl View<ShellState, (), ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Update>, state: &mut ShellState| {
            let (tx, rx) = xilem::tokio::sync::mpsc::channel(4);
            state.wireless.sender = Some(tx);
            async move {
                wireless::state::run(rx, move |u| proxy.message(u).is_ok()).await;
            }
        },
        |state: &mut ShellState, update| state.wireless.receive(update),
    )
}
pub(crate) fn panel(
    state: &ShellState,
    output: &super::state::ShellOutputInstance,
    bluetooth: bool,
) -> impl WidgetView<ShellState> + use<> {
    let State {
        snapshot,
        busy,
        error,
        ..
    } = &state.wireless;
    let radio = if bluetooth {
        &snapshot.bluetooth
    } else {
        &snapshot.wifi
    };
    let rows = radio
        .as_ref()
        .map(|r| {
            r.items
                .iter()
                .filter(|item| !bluetooth || item.paired || item.connected)
                .map(|item| {
                    let mut actions = vec![];
                    if bluetooth {
                        if item.connected {
                            actions.push((
                                "Disconnect".into(),
                                Command::DisconnectBluetooth(item.id.clone()),
                            ));
                        } else {
                            actions.push((
                                "Connect".into(),
                                Command::ConnectBluetooth(item.id.clone()),
                            ));
                        }
                    } else if item.connected {
                        actions.push((
                            "Disconnect".into(),
                            Command::DisconnectWifi(item.id.clone()),
                        ));
                    } else {
                        actions.push(("Connect".into(), Command::ConnectWifi(item.id.clone())));
                    }
                    Row {
                        id: item.id.clone(),
                        name: item.name.clone(),
                        detail: item.detail.clone(),
                        connected: item.connected,
                        known: item.paired,
                        secured: item.secured,
                        strength: item.strength,
                        actions,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let model = Panel {
        title: if bluetooth { "Bluetooth" } else { "Wi-Fi" },
        enabled: if state
            .wireless
            .sender
            .as_ref()
            .is_some_and(|s| !s.is_closed())
        {
            radio.as_ref().ok().map(|r| r.enabled)
        } else {
            None
        },
        blocked: radio.as_ref().is_ok_and(|r| r.blocked),
        busy: *busy,
        rows,
        selected: None,
        message: output
            .launch_error
            .clone()
            .or_else(|| error.clone())
            .or_else(|| radio.as_ref().err().cloned()),
    };
    let key = output.key().to_owned();
    let name = output.output_name.clone();
    let header = components::radio_panel::header(
        model.title,
        model.enabled,
        model.busy || model.enabled.is_none() || model.blocked,
        move |s: &mut ShellState, on| {
            s.wireless.request(if bluetooth {
                Command::BluetoothPower(on)
            } else {
                Command::WifiPower(on)
            })
        },
    );
    let body = components::radio_panel::body(
        state.appearance.palette,
        true,
        model,
        |_: &mut ShellState| {},
        |s: &mut ShellState, c| s.wireless.request(c),
        |_: &mut ShellState, _| {},
    );
    let footer = components::radio_panel::settings_link(
        state.appearance.palette,
        bluetooth,
        move |s: &mut ShellState| {
            super::launch::submit(
                s,
                &key,
                crate::ShellCommand::OpenWirelessSettings {
                    bluetooth,
                    output_name: name.clone(),
                }
                .wire_name(),
                None,
                false,
            );
        },
    );
    super::popup_group::group(
        if bluetooth { "Bluetooth" } else { "Wi-Fi" },
        super::popup::radio_panel(output, header, body, footer),
    )
}
