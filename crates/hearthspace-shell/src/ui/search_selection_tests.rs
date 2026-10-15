use super::*;
use crate::ui::{
    launcher,
    test_support::{Ui, ids},
    window_catalog::RunningWindow,
};
use masonry::{
    accesskit::Role,
    core::{
        TextEvent,
        keyboard::{Key, Modifiers, NamedKey},
    },
    widgets::SizedBox,
};
use std::{
    io::{Read, Write},
    os::unix::net::UnixListener,
    thread,
};
use xilem::{WidgetView, view::sized_box};

#[path = "search_accessibility_tests.rs"]
mod accessibility;

fn state() -> ShellState {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.catalog = Default::default();
    state.change_query("eDP-1", "Notes".into());
    state.windows = (1..=4)
        .map(|id| RunningWindow {
            id,
            title: format!("Notes {id}"),
            app_id: "editor".into(),
            focused: false,
        })
        .collect();
    reconcile(&mut state);
    state
}
fn view(state: &ShellState) -> impl WidgetView<ShellState, Widget = SizedBox> + use<> {
    sized_box(launcher::launcher_view(
        state,
        &state.outputs[0],
        launcher::results(state, &state.outputs[0]),
        None,
    ))
}
fn selected<V: WidgetView<ShellState>>(ui: &Ui<V>) -> Vec<String>
where
    V::Widget: Sized,
{
    ids(&ui.harness)
        .into_iter()
        .filter_map(|id| {
            let node = ui.harness.access_node(id)?;
            (node.role() == Role::Button && node.is_selected() == Some(true))
                .then(|| node.label().unwrap())
        })
        .collect()
}

#[test]
fn identity_survives_reordering_disappearing_selection_clamps_and_query_resets() {
    let mut state = state();
    step(&mut state, "eDP-1", 2);
    assert_eq!(state.outputs[0].selected_result, Some(EntryId::Window(3)));
    state.windows[2].focused = true;
    reconcile(&mut state);
    assert_eq!(state.outputs[0].selected_result_index, 0);
    assert_eq!(state.outputs[0].selected_result, Some(EntryId::Window(3)));
    step(&mut state, "eDP-1", 99);
    assert_eq!(state.outputs[0].selected_result, Some(EntryId::Window(4)));
    state.windows.retain(|window| window.id != 4);
    reconcile(&mut state);
    assert_eq!(state.outputs[0].selected_result, Some(EntryId::Window(2)));
    state.change_query("eDP-1", "Notes 1".into());
    reconcile(&mut state);
    assert_eq!(state.outputs[0].selected_result, Some(EntryId::Window(1)));
    state.change_query("eDP-1", "missing".into());
    reconcile(&mut state);
    assert_eq!(state.outputs[0].selected_result, None);
    step(&mut state, "eDP-1", 1);
    assert_eq!(state.outputs[0].selected_result, None);
}

#[test]
fn arrows_keep_typing_focus_show_selection_scroll_and_enter_activates_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state();
    state.command_socket = dir.path().join("control.sock");
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = String::new();
        stream.read_to_string(&mut request).unwrap();
        assert_eq!(request, "focus-window 3\n");
        stream.write_all(b"ok\n").unwrap();
    });
    let mut ui = Ui::new(state, view, (360, 176));
    let input = ui.harness.focused_widget_id().unwrap();
    assert_eq!(selected(&ui), ["Window: Notes 1"]);
    let before = ui.harness.render();
    for _ in 0..3 {
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        ui.dispatch(&view);
        assert_eq!(ui.harness.focused_widget_id(), Some(input));
    }
    assert_eq!(selected(&ui), ["Window: Notes 4"]);
    assert_ne!(ui.harness.render(), before);
    let last = ui
        .harness
        .access_node(ui.button("Window: Notes 4"))
        .unwrap()
        .bounding_box()
        .unwrap();
    assert!(
        last.y1 <= 176.0 - 12.0,
        "selected row should scroll into view: {last:?}"
    );
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowUp)));
    ui.dispatch(&view);
    assert_eq!(selected(&ui), ["Window: Notes 3"]);
    assert_eq!(ui.state.outputs[0].query, "Notes");
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].show_search);
    server.join().unwrap();
}

#[test]
fn tab_focus_tracks_selection_and_clipboard_edit_keeps_native_editor_actions() {
    let mut ui = Ui::new(state(), view, (360, 280));
    let input = ui.harness.focused_widget_id().unwrap();
    // The scroll portal is also a normal keyboard focus stop.
    for _ in 0..3 {
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
        ui.dispatch(&view);
    }
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Window: Notes 2"))
    );
    assert_eq!(selected(&ui), ["Window: Notes 2"]);
    ui.harness.focus_on(Some(input));
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::End)));
    ui.harness
        .process_text_event(TextEvent::ClipboardPaste(" 4".into()));
    ui.dispatch(&view);
    assert_eq!(ui.state.outputs[0].query, "Notes 4");
    assert_eq!(selected(&ui), ["Window: Notes 4"]);
    let node = ui.harness.access_node(input).unwrap();
    assert_eq!(node.role(), Role::TextInput);
    assert_eq!(node.label().as_deref(), Some("Search apps and windows..."));
    assert_eq!(node.value().as_deref(), Some("Notes 4"));
}

#[test]
fn long_queries_keep_search_header_and_results_inside_the_panel() {
    let mut state = state();
    let query = "long query ".repeat(100);
    state.windows[0].title.clone_from(&query);
    state.change_query("eDP-1", query.clone());
    reconcile(&mut state);
    let ui = Ui::new(state, view, (360, 176));
    let input = ui.harness.focused_widget_id().unwrap();
    let input_bounds = ui
        .harness
        .access_node(input)
        .unwrap()
        .bounding_box()
        .unwrap();
    let close_bounds = ui
        .harness
        .access_node(ui.button("Close search"))
        .unwrap()
        .bounding_box()
        .unwrap();
    let result_bounds = ui
        .harness
        .access_node(ui.button(&format!("Window: {query}")))
        .unwrap()
        .bounding_box()
        .unwrap();
    assert!(input_bounds.height() <= 34.0, "{input_bounds:?}");
    assert!(
        close_bounds.y1 <= input_bounds.y0,
        "{close_bounds:?} {input_bounds:?}"
    );
    assert!(
        result_bounds.y0 >= input_bounds.y1 && result_bounds.y1 <= 164.0,
        "{result_bounds:?}"
    );
}

#[test]
fn modified_arrows_do_not_change_result_selection_or_keyboard_focus() {
    let mut ui = Ui::new(state(), view, (360, 280));
    let input = ui.harness.focused_widget_id().unwrap();
    for focused in [input, ui.button("Window: Notes 1")] {
        ui.harness.focus_on(Some(focused));
        ui.dispatch(&view);
        for modifiers in [
            Modifiers::SHIFT,
            Modifiers::CONTROL,
            Modifiers::ALT,
            Modifiers::META,
        ] {
            let TextEvent::Keyboard(mut event) =
                TextEvent::key_down(Key::Named(NamedKey::ArrowDown))
            else {
                unreachable!()
            };
            event.modifiers = modifiers;
            ui.harness.process_text_event(TextEvent::Keyboard(event));
            ui.dispatch(&view);
            assert_eq!(selected(&ui), ["Window: Notes 1"]);
            assert_eq!(ui.harness.focused_widget_id(), Some(focused));
        }
    }
}

#[test]
fn external_clear_preserves_focus_and_ime_composition_never_moves_results() {
    use masonry::core::Ime;
    let mut ui = Ui::new(state(), view, (360, 280));
    let input = ui.harness.focused_widget_id().unwrap();
    ui.state.change_query("eDP-1", String::new());
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(input));
    ui.harness.process_text_event(TextEvent::Ime(Ime::Enabled));
    ui.harness
        .process_text_event(TextEvent::Ime(Ime::Preedit("Notes".into(), Some((5, 5)))));
    ui.dispatch(&view);
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].selected_result.is_none());
    // Winit ends the preedit before committing its text; mirror that sequence.
    ui.harness
        .process_text_event(TextEvent::Ime(Ime::Preedit(String::new(), None)));
    ui.harness
        .process_text_event(TextEvent::Ime(Ime::Commit("Notes".into())));
    ui.dispatch(&view);
    assert_eq!(ui.state.outputs[0].query, "Notes");
    assert_eq!(selected(&ui), ["Window: Notes 1"]);
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
    ui.dispatch(&view);
    assert_eq!(selected(&ui), ["Window: Notes 2"]);
    assert_eq!(ui.harness.focused_widget_id(), Some(input));
    assert_eq!(
        ui.harness.access_node(input).unwrap().value().as_deref(),
        Some("Notes")
    );
}
