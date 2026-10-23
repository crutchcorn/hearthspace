use super::*;
use masonry::core::{PointerButtonEvent, PointerEvent, PointerInfo, PointerState, PointerType};

fn touch_event() -> PointerButtonEvent {
    PointerButtonEvent {
        pointer: PointerInfo {
            pointer_type: PointerType::Touch,
            pointer_id: Some(masonry::core::pointer::PointerId::PRIMARY),
            persistent_device_id: None,
        },
        state: PointerState {
            position: (17.0, 17.0).into(),
            ..Default::default()
        },
        button: None,
    }
}

#[test]
fn touch_taps_activate_and_cancelled_or_moved_holds_do_not_open_menus() {
    let widget = TrayButton::new(
        Appearance {
            name: "App".into(),
            icon: true,
            ..Default::default()
        },
        Target::Item(key(1)),
    )
    .prepare();
    let mut harness = TestHarness::create_with_size(
        crate::ui::appearance::properties(hearthspace_ui_components::Palette::default()),
        widget,
        (34, 34),
    );
    let event = touch_event();
    harness.process_pointer_event(PointerEvent::Down(event.clone()));
    harness.process_pointer_event(PointerEvent::Up(event.clone()));
    let actions = std::iter::from_fn(|| harness.pop_action::<ButtonEvent>()).collect::<Vec<_>>();
    assert!(
        actions
            .iter()
            .any(|(event, _)| matches!(event.action, ButtonAction::Activate))
    );
    assert!(
        !actions
            .iter()
            .any(|(event, _)| matches!(event.action, ButtonAction::Context))
    );

    harness.process_pointer_event(PointerEvent::Down(event.clone()));
    harness.process_pointer_event(PointerEvent::Cancel(event.pointer));
    harness.animate_ms(0);
    harness.animate_ms(600);
    assert!(
        !std::iter::from_fn(|| harness.pop_action::<ButtonEvent>()).any(|(event, _)| matches!(
            event.action,
            ButtonAction::Activate | ButtonAction::Context
        ))
    );

    harness.process_pointer_event(PointerEvent::Down(event.clone()));
    harness.process_pointer_event(PointerEvent::Move(masonry::core::PointerUpdate {
        pointer: event.pointer,
        current: PointerState {
            position: (30.0, 17.0).into(),
            ..event.state.clone()
        },
        coalesced: vec![],
        predicted: vec![],
    }));
    harness.animate_ms(0);
    harness.animate_ms(600);
    harness.process_pointer_event(PointerEvent::Up(event));
    assert!(
        !std::iter::from_fn(|| harness.pop_action::<ButtonEvent>()).any(|(event, _)| matches!(
            event.action,
            ButtonAction::Activate | ButtonAction::Context
        ))
    );
}

#[test]
fn every_active_app_has_an_icon_without_a_container_button() {
    for vertical in [false, true] {
        let chain = crate::ui::frame_layout::focus::Chain::default();
        let strip = |state: &ShellState| {
            sized_box(tray_view::frame_strip(
                &state.outputs[0],
                state,
                vertical,
                &chain,
                false,
            ))
        };
        let mut state = state();
        state.tray.snapshot.items = (1..=12).map(item).collect();
        state.tray.snapshot.items[3].status = TrayStatus::Passive;
        let size = if vertical { (34, 500) } else { (500, 34) };
        let mut ui = Ui::new(state, &strip, size);
        let labels = |ui: &Ui<_>| {
            ids(&ui.harness)
                .into_iter()
                .rev()
                .filter_map(|id| {
                    let node = ui.harness.access_node(id)?;
                    (node.role() == Role::Button).then(|| node.label().unwrap().to_string())
                })
                .collect::<Vec<_>>()
        };
        let expected = (1..=12)
            .filter(|id| *id != 4)
            .map(|id| format!("App {id}"))
            .collect::<Vec<_>>();
        assert_eq!(labels(&ui), expected);
        ui.harness.focus_on(Some(name(&ui, "App 12")));
        press(&mut ui, NamedKey::ContextMenu);
        ui.dispatch(&strip);
        assert_eq!(ui.state.outputs[0].tray.item, Some(key(12)));
        ui.state.tray.snapshot.items.clear();
        tray_state::reconcile(&mut ui.state);
        ui.rebuild(&strip);
        ui.dispatch(&strip);
        assert!(labels(&ui).is_empty());
        assert!(!ui.state.outputs[0].tray.open);
    }
}

#[test]
fn touch_hold_on_app_icon_opens_its_menu_and_root_back_closes_it() {
    let icon =
        |state: &ShellState| sized_box(tray_button::icon("eDP-1", &state.tray.snapshot.items[0]));
    let mut ui = Ui::new(state(), &icon, (34, 34));
    let event = touch_event();
    ui.harness
        .process_pointer_event(PointerEvent::Down(event.clone()));
    ui.harness.animate_ms(0);
    ui.harness.animate_ms(600);
    ui.harness.process_pointer_event(PointerEvent::Up(event));
    ui.dispatch(&icon);
    assert!(ui.state.outputs[0].tray.open);
    assert_eq!(ui.state.outputs[0].tray.item, Some(key(1)));
    tray_state::back(&mut ui.state, "eDP-1");
    assert!(!ui.state.outputs[0].tray.open);
    assert_eq!(ui.state.outputs[0].tray.item, None);
}
