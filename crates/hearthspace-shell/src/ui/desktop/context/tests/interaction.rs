use super::*;

fn apps() -> Completed {
    Completed::OpenWith(vec![crate::desktop_items::actions::OpenWithApp {
        id: "music.desktop".into(),
        name: "Music".into(),
    }])
}

#[test]
fn loading_child_has_keyboard_back_and_cancellation_survives_reenabled_hover() {
    let (state, mut requests) = state(true, 600);
    let mut ui = Ui::new(state, panel, (340, 320));
    let trigger = named(&ui, "Open With…", Role::MenuItem);
    ui.harness.mouse_move_to(trigger);
    ui.dispatch(&panel);
    assert!(ui.state.desktop.menu.busy);
    assert!(ui.state.desktop.menu.submenu.is_some());
    requests.try_recv().unwrap();
    fn child(
        state: &ShellState,
    ) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
        sized_box(view::panel(state, true))
    }
    // Exercise the actual focused loading control, not a direct Back callback.
    let child_state = std::mem::replace(&mut ui.state, state_for_replacement());
    let mut submenu = Ui::new(child_state, child, (340, 112));
    assert_eq!(
        submenu.harness.focused_widget_id(),
        Some(named(&submenu, "Back", Role::MenuItem))
    );
    press(&mut submenu, NamedKey::ArrowLeft);
    submenu.dispatch(&child);
    assert!(submenu.state.desktop.menu.submenu.is_none());
    ui.state = submenu.state;
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    ui.harness.mouse_move_to(trigger);
    ui.dispatch(&panel);
    finished(&mut ui.state, 8, Ok(apps()));
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    assert!(ui.state.desktop.menu.submenu.is_none());
    assert!(
        matches!(requests.try_recv().unwrap(), task::Request::Command(command) if command == "desktop-context-parent 8")
    );
    assert!(
        requests.try_recv().is_err(),
        "completion must not start new discovery"
    );
    // Explicit keyboard activation can reopen without needing pointer motion.
    ui.activate(trigger);
    ui.dispatch(&panel);
    assert!(ui.state.desktop.menu.submenu.is_some());
}

fn state_for_replacement() -> ShellState {
    ShellState::new("/unused".into(), vec!["eDP-1".into()])
}

#[test]
fn returning_from_child_after_leaving_trigger_allows_the_next_hover() {
    let (state, mut requests) = state(true, 600);
    let mut ui = Ui::new(state, panel, (340, 320));
    let trigger = named(&ui, "Open With…", Role::MenuItem);
    ui.harness.mouse_move_to(trigger);
    ui.dispatch(&panel);
    requests.try_recv().unwrap();
    finished(&mut ui.state, 8, Ok(apps()));
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    ui.harness.mouse_move((-20.0, -20.0));
    ui.dispatch(&panel);
    button_event(&mut ui.state, 8, true, 0, ButtonAction::Back);
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    assert!(ui.state.desktop.menu.submenu.is_none());
    requests.try_recv().unwrap();
    ui.harness.mouse_move_to(trigger);
    ui.dispatch(&panel);
    assert!(ui.state.desktop.menu.submenu.is_some());
    assert!(matches!(
        requests.try_recv().unwrap(),
        task::Request::Context(context_actions::Request {
            action: Action::OpenWithOptions,
            ..
        })
    ));
}

#[test]
fn open_with_anchors_to_the_scrolled_row_and_right_enters_existing_child() {
    let (state, mut requests) = state(true, 130);
    let mut ui = Ui::new(state, panel, (260, 130));
    for _ in 0..6 {
        press(&mut ui, NamedKey::ArrowDown);
        ui.dispatch(&panel);
    }
    for _ in 0..5 {
        press(&mut ui, NamedKey::ArrowUp);
        ui.dispatch(&panel);
    }
    let trigger = named(&ui, "Open With…", Role::MenuItem);
    let y = ui
        .harness
        .get_widget_with_id(trigger)
        .ctx()
        .bounding_box()
        .y0;
    press(&mut ui, NamedKey::ArrowRight);
    ui.dispatch(&panel);
    let child = ui.state.desktop.menu.submenu.as_ref().unwrap();
    assert_eq!(child.row_y, (y - 6.0).round().max(0.0) as u32);
    assert_ne!(
        child.row_y, 34,
        "fixture must scroll Open With above its original row position"
    );
    requests.try_recv().unwrap();
    finished(&mut ui.state, 8, Ok(apps()));
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    press(&mut ui, NamedKey::ArrowRight);
    ui.dispatch(&panel);
    assert!(
        matches!(requests.try_recv().unwrap(), task::Request::Command(command) if command == "desktop-context-child 8")
    );
}
