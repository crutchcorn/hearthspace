use super::*;
use masonry::core::PointerButton;

fn event<V: WidgetView<ShellState, Widget: Sized>>(
    ui: &mut Ui<V>,
    event: TextEvent,
    build: &impl Fn(&ShellState) -> V,
) {
    ui.harness.process_text_event(event);
    ui.dispatch(build);
}

fn repeat(key: &Key) -> TextEvent {
    let TextEvent::Keyboard(mut event) = TextEvent::key_down(key.clone()) else {
        unreachable!();
    };
    event.repeat = true;
    TextEvent::Keyboard(event)
}

fn saved(state: &mut ShellState, directory: &std::path::Path, success: bool) {
    if success {
        let saved = catalog(directory, true);
        state.catalog = saved.clone();
        finished(state, 8, Ok(Completed::GpuPreference(saved)));
    } else {
        finished(state, 8, Err("Could not save. Try again.".into()));
    }
}

fn assert_write(requests: &mut tokio::sync::mpsc::Receiver<task::Request>, enabled: bool) {
    assert!(matches!(
        requests.try_recv().unwrap(),
        task::Request::Context(context_actions::Request {
            generation: 8,
            action: Action::GpuPreference(value),
        }) if value == enabled
    ));
    assert!(
        requests.try_recv().is_err(),
        "one sequence must enqueue one save"
    );
}

#[test]
fn checkbox_requires_owned_release_and_held_enter_or_space_enqueues_one_save() {
    for key in [Key::Named(NamedKey::Enter), Key::Character(" ".into())] {
        let directory = tempfile::tempdir().unwrap();
        let (state, mut requests) = app_state(directory.path(), false);
        let mut ui = Ui::new(state, panel, (340, 360));
        let checkbox = named(&ui, LABEL, Role::MenuItemCheckBox);
        ui.harness.focus_on(Some(checkbox));
        ui.dispatch(&panel);
        event(&mut ui, TextEvent::key_up(key.clone()), &panel);
        assert!(requests.try_recv().is_err());
        event(&mut ui, TextEvent::key_down(key.clone()), &panel);
        for _ in 0..4 {
            event(&mut ui, repeat(&key), &panel);
            assert!(requests.try_recv().is_err());
            assert!(!ui.state.desktop.menu.busy);
            assert_eq!(ui.harness.focused_widget_id(), Some(checkbox));
        }
        event(&mut ui, TextEvent::key_up(key.clone()), &panel);
        assert_write(&mut requests, true);
        assert!(ui.state.desktop.menu.busy);
        assert_eq!(
            ui.harness.focused_widget_id(),
            Some(named(&ui, "Close", Role::MenuItem))
        );
        saved(&mut ui.state, directory.path(), true);
        ui.rebuild(&panel);
        ui.dispatch(&panel);
        assert_eq!(ui.harness.focused_widget_id(), Some(checkbox));
        event(&mut ui, TextEvent::key_up(key.clone()), &panel);
        assert!(requests.try_recv().is_err());
        assert!(ui.state.desktop.menu.current.is_some());
        assert_eq!(
            ui.harness.access_node(checkbox).unwrap().toggled(),
            Some(Toggled::True)
        );
        // Only another complete sequence may change the committed value again.
        event(&mut ui, TextEvent::key_down(key.clone()), &panel);
        event(&mut ui, TextEvent::key_up(key), &panel);
        assert_write(&mut requests, false);
    }
}

#[test]
fn pending_close_key_cannot_transfer_to_checkbox_after_save_or_failure() {
    for success in [true, false] {
        for key in [Key::Named(NamedKey::Enter), Key::Character(" ".into())] {
            let directory = tempfile::tempdir().unwrap();
            let (state, mut requests) = app_state(directory.path(), false);
            let mut ui = Ui::new(state, panel, (340, 360));
            let checkbox = named(&ui, LABEL, Role::MenuItemCheckBox);
            ui.harness
                .mouse_click_on(checkbox, Some(PointerButton::Primary));
            ui.dispatch(&panel);
            assert_write(&mut requests, true);
            let close = named(&ui, "Close", Role::MenuItem);
            assert_eq!(ui.harness.focused_widget_id(), Some(close));
            event(&mut ui, TextEvent::key_down(key.clone()), &panel);
            event(&mut ui, repeat(&key), &panel);
            assert!(ui.state.desktop.menu.current.is_some());
            assert!(requests.try_recv().is_err());
            saved(&mut ui.state, directory.path(), success);
            ui.rebuild(&panel);
            ui.dispatch(&panel);
            assert_eq!(ui.harness.focused_widget_id(), Some(checkbox));
            event(&mut ui, repeat(&key), &panel);
            event(&mut ui, TextEvent::key_up(key.clone()), &panel);
            assert!(
                requests.try_recv().is_err(),
                "Close must not transfer its held key"
            );
            assert!(ui.state.desktop.menu.current.is_some());
            assert_eq!(ui.state.desktop.menu.error.is_some(), !success);
            event(&mut ui, TextEvent::key_down(key.clone()), &panel);
            event(&mut ui, TextEvent::key_up(key), &panel);
            assert_write(&mut requests, !success);
        }
    }
}

#[test]
fn pointer_save_cancels_keyboard_press_across_disabled_and_reenabled_checkbox() {
    for success in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let (state, mut requests) = app_state(directory.path(), false);
        let mut ui = Ui::new(state, panel, (340, 360));
        let checkbox = named(&ui, LABEL, Role::MenuItemCheckBox);
        ui.harness.focus_on(Some(checkbox));
        ui.dispatch(&panel);
        let key = Key::Named(NamedKey::Enter);
        event(&mut ui, TextEvent::key_down(key.clone()), &panel);
        ui.harness
            .mouse_click_on(checkbox, Some(PointerButton::Primary));
        ui.dispatch(&panel);
        assert_write(&mut requests, true);
        event(&mut ui, repeat(&key), &panel);
        assert!(requests.try_recv().is_err());
        assert!(ui.state.desktop.menu.current.is_some());
        saved(&mut ui.state, directory.path(), success);
        ui.rebuild(&panel);
        ui.dispatch(&panel);
        event(&mut ui, repeat(&key), &panel);
        event(&mut ui, TextEvent::key_up(key), &panel);
        assert!(requests.try_recv().is_err());
        assert!(!ui.state.desktop.menu.busy);
        assert!(ui.state.desktop.menu.current.is_some());
        assert_eq!(
            ui.harness.access_node(checkbox).unwrap().toggled(),
            Some(if success {
                Toggled::True
            } else {
                Toggled::False
            })
        );
    }
}

#[test]
fn leaving_and_returning_focus_does_not_rearm_a_held_checkbox_key() {
    let directory = tempfile::tempdir().unwrap();
    let (state, mut requests) = app_state(directory.path(), false);
    let mut ui = Ui::new(state, panel, (340, 360));
    let checkbox = named(&ui, LABEL, Role::MenuItemCheckBox);
    ui.harness.focus_on(Some(checkbox));
    ui.dispatch(&panel);
    let key = Key::Named(NamedKey::Enter);
    event(&mut ui, TextEvent::key_down(key.clone()), &panel);
    let properties = named(&ui, "Properties", Role::MenuItem);
    ui.harness.focus_on(Some(properties));
    ui.dispatch(&panel);
    event(&mut ui, repeat(&key), &panel);
    ui.harness.focus_on(Some(checkbox));
    ui.dispatch(&panel);
    event(&mut ui, repeat(&key), &panel);
    event(&mut ui, TextEvent::key_up(key), &panel);
    assert!(requests.try_recv().is_err());
    assert!(!ui.state.desktop.menu.busy);
    assert!(ui.state.desktop.menu.current.is_some());
}

#[test]
fn pending_close_accepts_its_own_complete_key_sequence_once() {
    let directory = tempfile::tempdir().unwrap();
    let (state, mut requests) = app_state(directory.path(), false);
    let mut ui = Ui::new(state, panel, (340, 360));
    let checkbox = named(&ui, LABEL, Role::MenuItemCheckBox);
    ui.harness
        .mouse_click_on(checkbox, Some(PointerButton::Primary));
    ui.dispatch(&panel);
    assert_write(&mut requests, true);
    let key = Key::Named(NamedKey::Enter);
    event(&mut ui, TextEvent::key_down(key.clone()), &panel);
    event(&mut ui, repeat(&key), &panel);
    assert!(requests.try_recv().is_err());
    assert!(ui.state.desktop.menu.current.is_some());
    event(&mut ui, TextEvent::key_up(key), &panel);
    assert!(
        matches!(requests.try_recv().unwrap(), task::Request::Command(command)
        if command == "desktop-context-close 8")
    );
    assert!(requests.try_recv().is_err());
    assert!(ui.state.desktop.menu.current.is_none());
    saved(&mut ui.state, directory.path(), true);
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    assert!(ui.state.desktop.menu.current.is_none());
    assert!(requests.try_recv().is_err());
}
