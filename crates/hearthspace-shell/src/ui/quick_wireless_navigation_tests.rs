//! Tray navigation and scrolling exercise the real popup and launch queue.
use super::*;
use crate::ui::{
    launch::tests::{finish_next, launch_test_queue},
    test_support::{Ui, ids},
};
use hearthspace_desktop::wireless::{Item, Radio};
use masonry::{accesskit::Role, core::WidgetId, kurbo::Rect};

fn view(
    s: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(panel(s, &s.outputs[0]))
}
fn bounds<V: WidgetView<ShellState, Widget = masonry::widgets::SizedBox>>(
    ui: &Ui<V>,
    id: WidgetId,
) -> Rect {
    let w = ui.harness.get_widget_with_id(id);
    w.ctx()
        .window_transform()
        .transform_rect_bbox(w.ctx().border_box())
}
#[test]
fn connection_list_scrolls_below_fixed_toggle_and_above_fixed_settings_link() {
    let mut s = ShellState::new("/unused".into(), vec!["A".into()]);
    s.toggle_quick_panel("A", Kind::Wifi);
    s.wireless.snapshot.wifi = Ok(Radio {
        enabled: true,
        items: (0..30)
            .map(|i| Item {
                id: format!("ap{i}"),
                name: format!("Network {i}"),
                strength: Some(50),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    });
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    s.wireless.sender = Some(tx);
    let mut ui = Ui::new(s, view, (320, 400));
    ui.dispatch(&view);
    let switch = ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness
                .access_node(*id)
                .is_some_and(|n| n.role() == Role::Switch)
        })
        .unwrap();
    let scroll = ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|n| {
                n.role() == Role::ScrollView && n.scroll_y_max().is_some_and(|v| v > 0.)
            })
        })
        .unwrap();
    let settings = ui.button("WiFi settings");
    let toggle_rect = bounds(&ui, switch);
    let settings_rect = bounds(&ui, settings);
    let scroll_rect = bounds(&ui, scroll);
    assert!(
        scroll_rect.y0 >= toggle_rect.y1,
        "scrollbar must start below toggle: {scroll_rect:?} {toggle_rect:?}"
    );
    assert!(
        scroll_rect.y1 <= settings_rect.y0,
        "settings link must remain outside scrolling content"
    );
    let first = ui.button("Connect to Network 0");
    let before = bounds(&ui, first);
    ui.harness
        .scroll_into_view(ui.button("Connect to Network 29"));
    ui.dispatch(&view);
    assert!(bounds(&ui, first).y0 < before.y0);
    assert_eq!(bounds(&ui, switch), toggle_rect);
    assert_eq!(bounds(&ui, settings), settings_rect);
    assert!(
        rx.try_recv().is_err(),
        "scrolling must never connect a network"
    );
}
#[test]
fn tray_settings_links_launch_exact_page_on_output_and_preserve_errors() {
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
    };
    for (kind, label, wire) in [
        (Kind::Wifi, "WiFi settings", "open-wifi-settings 41\n"),
        (
            Kind::Bluetooth,
            "Bluetooth settings",
            "open-bluetooth-settings 41\n",
        ),
    ] {
        for accepted in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let socket = dir.path().join("control");
            let listener = UnixListener::bind(&socket).unwrap();
            let mut s = ShellState::new(socket, vec!["A".into()]);
            s.toggle_quick_panel("A", kind);
            let mut queue = launch_test_queue(&mut s);
            let mut ui = Ui::new(s, view, (320, 400));
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut command = String::new();
                BufReader::new(&stream).read_line(&mut command).unwrap();
                let envelope =
                    hearthspace_ipc::transport::launch::Envelope::decode(command.as_bytes())
                        .unwrap()
                        .unwrap();
                assert_eq!(envelope.command, wire.trim_end());
                stream
                    .write_all(if accepted {
                        b"ok\n"
                    } else {
                        b"err Settings unavailable\n"
                    })
                    .unwrap();
            });
            let button = ui.button(label);
            ui.harness.focus_on(Some(button));
            use masonry::core::{
                TextEvent,
                keyboard::{Key, NamedKey},
            };
            ui.harness
                .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
            ui.harness
                .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
            ui.dispatch(&view);
            finish_next(&mut ui.state, &mut queue);
            server.join().unwrap();
            ui.rebuild(&view);
            ui.dispatch(&view);
            assert_eq!(ui.state.outputs[0].show_controls, !accepted);
            if !accepted {
                assert_eq!(
                    ui.state.outputs[0].launch_error.as_deref(),
                    Some("Settings unavailable")
                );
            }
        }
    }
}
#[test]
fn bluetooth_tray_only_exposes_connect_and_disconnect_for_known_devices() {
    let mut s = ShellState::new("/unused".into(), vec!["A".into()]);
    s.toggle_quick_panel("A", Kind::Bluetooth);
    s.wireless.snapshot.bluetooth = Ok(Radio {
        enabled: true,
        items: vec![
            Item {
                id: "paired".into(),
                name: "Keyboard".into(),
                paired: true,
                ..Default::default()
            },
            Item {
                id: "nearby".into(),
                name: "Stranger".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    s.wireless.sender = Some(tx);
    let ui = Ui::new(s, view, (320, 400));
    ui.button("Connect Keyboard");
    ui.button("Bluetooth settings");
    let buttons: Vec<_> = ids(&ui.harness)
        .into_iter()
        .filter_map(|id| {
            ui.harness
                .access_node(id)
                .filter(|n| n.role() == Role::Button)
                .and_then(|n| n.label())
        })
        .collect();
    assert_eq!(
        buttons.len(),
        2,
        "tray must only offer connection and Settings: {buttons:?}"
    );
}

#[test]
fn switching_radio_panels_cannot_transfer_a_held_settings_link_press() {
    use masonry::core::PointerButton;
    let mut s = ShellState::new("/unused".into(), vec!["A".into()]);
    s.toggle_quick_panel("A", Kind::Wifi);
    let mut queue = launch_test_queue(&mut s);
    let mut ui = Ui::new(s, view, (320, 400));
    ui.harness.mouse_move_to(ui.button("WiFi settings"));
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.dispatch(&view);
    ui.state.toggle_quick_panel("A", Kind::Bluetooth);
    ui.rebuild(&view);
    ui.dispatch(&view);
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert!(
        queue.try_recv().is_err(),
        "release must not launch a replacement Settings page"
    );
    ui.activate(ui.button("Bluetooth settings"));
    ui.dispatch(&view);
    assert!(
        queue.try_recv().is_ok(),
        "a fresh activation must still work"
    );
}
