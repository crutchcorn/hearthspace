//! Bind the shared radio controls to this app's asynchronous service worker.
use crate::state::SettingsApp;
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
pub(crate) fn task() -> impl View<SettingsApp, (), ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Update>, state: &mut SettingsApp| {
            let (tx, rx) = xilem::tokio::sync::mpsc::channel(4);
            state.wireless.sender = Some(tx);
            async move {
                wireless::state::run(rx, move |u| proxy.message(u).is_ok()).await;
            }
        },
        |state: &mut SettingsApp, update| state.wireless.receive(update),
    )
}
pub(crate) fn panel(state: &SettingsApp, bluetooth: bool) -> impl WidgetView<SettingsApp> + use<> {
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
                .map(|item| {
                    let mut actions = vec![];
                    if bluetooth {
                        if item.connected {
                            actions.push((
                                "Disconnect".into(),
                                Command::DisconnectBluetooth(item.id.clone()),
                            ));
                        } else if item.paired {
                            actions.push((
                                "Connect".into(),
                                Command::ConnectBluetooth(item.id.clone()),
                            ));
                        } else {
                            actions.push(("Pair".into(), Command::PairBluetooth(item.id.clone())));
                        }
                        if item.paired {
                            actions
                                .push(("Forget".into(), Command::ForgetBluetooth(item.id.clone())));
                        }
                    } else if item.connected {
                        actions.push((
                            "Disconnect".into(),
                            Command::DisconnectWifi(item.id.clone()),
                        ));
                    } else {
                        actions.push(("Connect".into(), Command::ConnectWifi(item.id.clone())));
                    }
                    if !bluetooth {
                        actions.push(("Forget".into(), Command::ForgetWifi(item.id.clone())));
                    }
                    Row {
                        id: item.id.clone(),
                        name: item.name.clone(),
                        detail: item.detail.clone(),
                        connected: item.connected,
                        known: item.paired,
                        secured: item.secured,
                        actions,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    components::radio_panel::view(
        state.palette(),
        Panel {
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
            selected: state.wireless.details.clone(),
            message: error.clone().or_else(|| radio.as_ref().err().cloned()),
        },
        move |s: &mut SettingsApp, on| {
            s.wireless.request(if bluetooth {
                Command::BluetoothPower(on)
            } else {
                Command::WifiPower(on)
            })
        },
        move |s: &mut SettingsApp| {
            s.wireless.request(if bluetooth {
                Command::ScanBluetooth
            } else {
                Command::ScanWifi
            })
        },
        |s: &mut SettingsApp, c| s.wireless.request(c),
        |s: &mut SettingsApp, id| s.wireless.details = id,
    )
}
