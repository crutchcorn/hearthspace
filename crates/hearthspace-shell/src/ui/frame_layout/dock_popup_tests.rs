use super::*;
use crate::ui::tray_widget::ButtonAction;

#[test]
fn merged_menu_anchors_to_apps_on_the_initiating_output_and_delivers_once() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    let (send, mut receive) = tokio::sync::mpsc::channel(4);
    state.frame_layout.requests = Some(send);
    let item = state.tray.snapshot.items[0].clone();
    dock::tray::event(
        &mut state,
        "DP-1",
        "steam_steam.desktop",
        &item.key,
        "/Menu",
        ButtonAction::Context,
    );
    assert!(!state.outputs[1].tray.open);
    let request = receive.try_recv().unwrap();
    let anchor: serde_json::Value =
        serde_json::from_str(request.command.strip_prefix("frame-popup-anchor ").unwrap()).unwrap();
    assert_eq!(anchor["output"], "DP-1");
    assert_eq!(anchor["target"]["id"], "apps");
    assert_eq!(
        anchor["app_id"],
        hearthspace_common::tray_menu_app_id_for_output("DP-1")
    );
    let now = std::time::Instant::now();
    let opening = request.open.unwrap().acknowledge(1, now);
    opening.clone().deliver(&mut state, now);
    assert!(state.outputs[1].tray.open);
    assert!(!state.outputs[0].tray.open);
    tray_state::dismiss(&mut state, "DP-1");
    opening.deliver(&mut state, now);
    assert!(!state.outputs[1].tray.open);
}

#[test]
fn anchor_reply_cannot_open_reconnected_removed_replaced_or_hidden_menu() {
    for change in ["reconnect", "remove", "path", "hidden", "closed"] {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, _worker) = fixture(dir.path());
        let (send, mut receive) = tokio::sync::mpsc::channel(4);
        state.frame_layout.requests = Some(send);
        let item = state.tray.snapshot.items[0].clone();
        dock::tray::event(
            &mut state,
            "eDP-1",
            "steam_steam.desktop",
            &item.key,
            "/Menu",
            ButtonAction::Context,
        );
        let now = std::time::Instant::now();
        let opening = receive
            .try_recv()
            .unwrap()
            .open
            .unwrap()
            .acknowledge(1, now);
        match change {
            "reconnect" => state.tray.snapshot.items[0].key.generation += 1,
            "remove" => state.tray.snapshot.items.clear(),
            "path" => state.tray.snapshot.items[0].menu_path = Some("/Replacement".into()),
            "hidden" => state.frame_layout.snapshot.outputs[0]
                .layout
                .set_placement(&LayoutTarget::widget("apps"), ObjectPlacement::Hidden),
            "closed" => state.windows.clear(),
            _ => unreachable!(),
        }
        opening.deliver(&mut state, now);
        assert!(!state.outputs[0].tray.open, "{change}");
    }
}

#[test]
fn held_and_queued_context_actions_cannot_open_a_reconnected_item() {
    let dir = tempfile::tempdir().unwrap();
    let (state, _worker) = fixture(dir.path());
    let mut ui = Ui::new(state, view, (500, 300));
    let button = ui.button("Switch to Steam");
    ui.harness.mouse_move_to(button);
    ui.harness
        .mouse_button_press(Some(PointerButton::Secondary));
    ui.dispatch(&view);
    ui.state.tray.snapshot.items[0].key.generation += 1;
    ui.rebuild(&view);
    ui.harness
        .mouse_button_release(Some(PointerButton::Secondary));
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].tray.open);
    ui.harness.process_access_event(ActionRequest {
        action: Action::ShowContextMenu,
        target_tree: TreeId::ROOT,
        target_node: button.to_raw().into(),
        data: None,
    });
    ui.state.tray.snapshot.items[0].key.generation += 1;
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].tray.open);
}

#[test]
fn touch_hold_uses_the_same_menu_without_also_focusing_or_launching() {
    use masonry::core::{PointerButtonEvent, PointerEvent, PointerInfo, PointerState, PointerType};
    let dir = tempfile::tempdir().unwrap();
    let (state, _worker) = fixture(dir.path());
    let mut ui = Ui::new(state, view, (500, 300));
    let bounds = ui
        .harness
        .access_node(ui.button("Switch to Steam"))
        .unwrap()
        .bounding_box()
        .unwrap();
    let event = PointerButtonEvent {
        pointer: PointerInfo {
            pointer_type: PointerType::Touch,
            pointer_id: Some(masonry::core::pointer::PointerId::PRIMARY),
            persistent_device_id: None,
        },
        state: PointerState {
            position: ((bounds.x0 + bounds.x1) / 2.0, (bounds.y0 + bounds.y1) / 2.0).into(),
            ..Default::default()
        },
        button: None,
    };
    ui.harness
        .process_pointer_event(PointerEvent::Down(event.clone()));
    ui.harness.animate_ms(0);
    ui.harness.animate_ms(600);
    ui.harness.process_pointer_event(PointerEvent::Up(event));
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].tray.open);
    assert!(
        !ui.state.outputs[0].show_search,
        "a stray focus/launch would fail on the unused socket"
    );
}
