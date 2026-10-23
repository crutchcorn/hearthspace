use super::*;

#[test]
fn original_tray_menu_anchors_to_system_tray_on_its_output_and_delivers_once() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    let (send, mut receive) = tokio::sync::mpsc::channel(4);
    state.frame_layout.requests = Some(send);
    let item = state.tray.snapshot.items[0].clone();
    state.favorites.store.add("steam_steam.desktop").unwrap();
    tray_state::open_menu(&mut state, "DP-1", &item.key);
    assert!(!state.outputs[1].tray.open);
    let request = receive.try_recv().unwrap();
    let anchor: serde_json::Value =
        serde_json::from_str(request.command.strip_prefix("frame-popup-anchor ").unwrap()).unwrap();
    assert_eq!(anchor["output"], "DP-1");
    assert_eq!(anchor["target"]["id"], "tray");
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
fn pending_tray_menu_still_opens_after_the_dock_app_closes() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    let (send, mut receive) = tokio::sync::mpsc::channel(4);
    state.frame_layout.requests = Some(send);
    let item = state.tray.snapshot.items[0].clone();
    tray_state::open_menu(&mut state, "eDP-1", &item.key);
    let now = std::time::Instant::now();
    let opening = receive
        .try_recv()
        .unwrap()
        .open
        .unwrap()
        .acknowledge(1, now);
    state.windows.clear();
    opening.deliver(&mut state, now);
    assert!(state.outputs[0].tray.open);
    assert_eq!(state.outputs[0].tray.item, Some(item.key));
    assert_eq!(state.outputs[0].tray.menu_path, item.menu_path);
}

#[test]
fn held_and_queued_context_actions_cannot_open_a_reconnected_item() {
    let dir = tempfile::tempdir().unwrap();
    let (state, _worker) = fixture(dir.path());
    let mut ui = Ui::new(state, view, (500, 300));
    let button = ui.button("Localized status title");
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
fn touch_hold_on_original_tray_opens_menu_without_focusing_or_launching_dock_app() {
    use masonry::core::{PointerButtonEvent, PointerEvent, PointerInfo, PointerState, PointerType};
    let dir = tempfile::tempdir().unwrap();
    let (state, _worker) = fixture(dir.path());
    let mut ui = Ui::new(state, view, (500, 300));
    let bounds = ui
        .harness
        .access_node(ui.button("Localized status title"))
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
