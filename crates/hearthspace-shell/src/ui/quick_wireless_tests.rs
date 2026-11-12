//! Production panels dispatch intent only, and keep measured state on errors.
use super::*;
use crate::ui::test_support::{Ui, ids};
use hearthspace_desktop::wireless::{Command, Item, Radio, state::Update};
use masonry::accesskit::{Role, Toggled};
fn view(
    s: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(panel(s, &s.outputs[0]))
}
#[test]
fn wifi_connect_uses_observed_identity_and_waits_for_service_result() {
    let mut s = ShellState::new("/unused".into(), vec!["A".into()]);
    s.toggle_quick_panel("A", Kind::Wifi);
    s.wireless.snapshot.wifi = Ok(Radio {
        enabled: true,
        blocked: false,
        items: vec![Item {
            id: "owner|ap".into(),
            name: "Fixture network".into(),
            detail: "Secured".into(),
            ..Default::default()
        }],
    });
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    s.wireless.sender = Some(tx);
    let mut ui = Ui::new(s, view, (360, 440));
    let connect = ui.button("Connect");
    ui.activate(connect);
    ui.dispatch(&view);
    assert!(matches!(rx.try_recv(),Ok(Command::ConnectWifi(id)) if id=="owner|ap"));
    assert!(ui.state.wireless.busy);
    assert!(!ui.state.wireless.snapshot.wifi.as_ref().unwrap().items[0].connected);
    let snapshot = ui.state.wireless.snapshot.clone();
    ui.state.wireless.receive(Update {
        snapshot,
        finished: true,
        error: Some("Connection was cancelled.".into()),
    });
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(!ui.state.wireless.busy);
    assert!(ui.state.outputs[0].show_controls);
}
#[test]
fn bluetooth_switch_keeps_confirmed_state_when_service_rejects_toggle() {
    let mut s = ShellState::new("/unused".into(), vec!["A".into()]);
    s.toggle_quick_panel("A", Kind::Bluetooth);
    s.wireless.snapshot.bluetooth = Ok(Radio {
        enabled: true,
        ..Default::default()
    });
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    s.wireless.sender = Some(tx);
    let mut ui = Ui::new(s, view, (360, 440));
    let switch = ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness
                .access_node(*id)
                .is_some_and(|n| n.role() == Role::Switch)
        })
        .unwrap();
    ui.activate(switch);
    ui.dispatch(&view);
    assert!(matches!(rx.try_recv(), Ok(Command::BluetoothPower(false))));
    let snapshot = ui.state.wireless.snapshot.clone();
    ui.state.wireless.receive(Update {
        snapshot,
        finished: true,
        error: Some("Permission denied.".into()),
    });
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(
        ui.harness.access_node(switch).unwrap().toggled(),
        Some(Toggled::True)
    );
}
#[test]
fn a_removed_network_cannot_transfer_its_held_press_to_a_replacement_row() {
    use masonry::core::PointerButton;
    let mut s = ShellState::new("/unused".into(), vec!["A".into()]);
    s.toggle_quick_panel("A", Kind::Wifi);
    s.wireless.snapshot.wifi = Ok(Radio {
        enabled: true,
        blocked: false,
        items: vec![Item {
            id: "first".into(),
            name: "First network".into(),
            ..Default::default()
        }],
    });
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    s.wireless.sender = Some(tx);
    let mut ui = Ui::new(s, view, (360, 440));
    let first = ui.button("Connect");
    ui.harness.mouse_move_to(first);
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.dispatch(&view);
    ui.state.wireless.snapshot.wifi.as_mut().unwrap().items[0] = Item {
        id: "replacement".into(),
        name: "Replacement network".into(),
        ..Default::default()
    };
    ui.rebuild(&view);
    ui.dispatch(&view);
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert!(
        rx.try_recv().is_err(),
        "release must not connect a replacement network"
    );
    let current = ui.button("Connect");
    ui.activate(current);
    ui.dispatch(&view);
    assert!(matches!(rx.try_recv(),Ok(Command::ConnectWifi(id)) if id=="replacement"));
}

#[test]
fn radio_panels_render_in_both_palettes_at_narrow_widths_without_touching_services() {
    use hearthspace_common::{Appearance, ColorMode};
    use hearthspace_ui_components::Palette;
    for (mode, theme) in [(ColorMode::Light, "light"), (ColorMode::Dark, "dark")] {
        for (kind, name) in [(Kind::Wifi, "wifi"), (Kind::Bluetooth, "bluetooth")] {
            for width in [320, 360] {
                let mut s = ShellState::new("/unused".into(), vec!["A".into()]);
                s.appearance.update(Palette::new(Appearance {
                    color_mode: mode,
                    ..Default::default()
                }));
                s.toggle_quick_panel("A", kind);
                let radio = Radio {
                    enabled: true,
                    blocked: false,
                    items: vec![
                        Item {
                            id: "one".into(),
                            name: "Studio headphones and keyboard".into(),
                            detail: "Connected".into(),
                            connected: true,
                            paired: true,
                        },
                        Item {
                            id: "two".into(),
                            name: "Hearthspace guest network".into(),
                            detail: "Available nearby".into(),
                            connected: false,
                            paired: false,
                        },
                    ],
                };
                if kind == Kind::Wifi {
                    s.wireless.snapshot.wifi = Ok(radio)
                } else {
                    s.wireless.snapshot.bluetooth = Ok(radio)
                }
                let (tx, _rx) = tokio::sync::mpsc::channel(4);
                s.wireless.sender = Some(tx);
                let mut ui = Ui::new(s, view, (width, 400));
                let refresh = ui.button("Refresh devices");
                ui.harness.focus_on(Some(refresh));
                assert_eq!(ui.harness.focused_widget_id(), Some(refresh));
                let image = ui.harness.render();
                assert_eq!(image.dimensions(), (width, 400));
                if let Some(base) = std::env::var_os("HEARTHSPACE_SHELL_EVIDENCE") {
                    let dir = std::path::PathBuf::from(base).join("wireless-panels");
                    std::fs::create_dir_all(&dir).unwrap();
                    image
                        .save(dir.join(format!("{name}-{theme}-{width}.png")))
                        .unwrap();
                }
            }
        }
    }
}
