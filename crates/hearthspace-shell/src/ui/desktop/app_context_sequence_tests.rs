//! Stable window identity owns presses even when its app and title are unchanged.
use super::*;
use crate::ui::window_catalog::RunningWindow;

#[derive(Clone)]
enum Gesture {
    Key(Key),
    Pointer(PointerButton),
}

#[test]
fn same_label_window_replacement_cancels_activation_and_context_sequences() {
    for gesture in [
        Gesture::Key(Key::Named(NamedKey::Enter)),
        Gesture::Key(Key::Character(" ".into())),
        Gesture::Key(Key::Named(NamedKey::ContextMenu)),
        Gesture::Pointer(PointerButton::Primary),
        Gesture::Pointer(PointerButton::Secondary),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (mut state, mut requests) = fixture(directory.path());
        state.windows = vec![RunningWindow {
            id: 1,
            title: "Notes".into(),
            app_id: "editor.desktop".into(),
            focused: false,
        }];
        state.change_query("eDP-1", "Notes".into());
        let mut ui = Ui::new(state, apps, (650, 560));
        let first = ui.button("Window: Notes");
        match &gesture {
            Gesture::Key(key) => {
                ui.harness.focus_on(Some(first));
                ui.dispatch(&apps);
                ui.harness
                    .process_text_event(TextEvent::key_down(key.clone()));
            }
            Gesture::Pointer(button) => {
                ui.harness.mouse_move_to(first);
                ui.harness.mouse_button_press(Some(*button));
            }
        }
        ui.dispatch(&apps);
        assert!(requests.try_recv().is_err());
        // The vector slot, label and app context remain identical. Only the
        // native window identity changes while the original sequence is held.
        ui.state.windows[0].id = 2;
        ui.rebuild(&apps);
        ui.dispatch(&apps);
        let replacement = ui.button("Window: Notes");
        assert_eq!(
            replacement, first,
            "exercise the retained positional widget"
        );
        match &gesture {
            Gesture::Key(key) => {
                let TextEvent::Keyboard(mut repeat) = TextEvent::key_down(key.clone()) else {
                    unreachable!();
                };
                repeat.repeat = true;
                ui.harness.process_text_event(TextEvent::Keyboard(repeat));
                ui.dispatch(&apps);
                ui.harness
                    .process_text_event(TextEvent::key_up(key.clone()));
            }
            Gesture::Pointer(button) => ui.harness.mouse_button_release(Some(*button)),
        }
        ui.dispatch(&apps);
        assert!(
            requests.try_recv().is_err(),
            "replacement must not inherit an app-menu gesture"
        );
        assert!(
            ui.state.outputs[0].launch_error.is_none(),
            "replacement must not receive focus-window"
        );
        assert!(ui.state.outputs[0].show_search);
        assert!(ui.state.outputs[0].launch_pending.is_none());
        // Cancellation does not disable the replacement's next fresh gesture.
        ui.harness.focus_on(Some(replacement));
        ui.dispatch(&apps);
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ContextMenu)));
        ui.dispatch(&apps);
        assert!(requests.try_recv().is_err());
        ui.harness
            .process_text_event(TextEvent::key_up(Key::Named(NamedKey::ContextMenu)));
        ui.dispatch(&apps);
        assert!(matches!(
            requests.try_recv().unwrap(),
            Request::AppContext(_)
        ));
        assert!(requests.try_recv().is_err());
    }
}
