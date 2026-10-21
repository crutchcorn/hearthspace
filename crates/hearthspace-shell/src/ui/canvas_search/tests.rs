use super::*;
use crate::ui::test_support::Ui;
use masonry::{
    accesskit::Role,
    core::{
        TextEvent,
        keyboard::{Key, Modifiers, NamedKey},
    },
};
use xilem::{WidgetView, view::sized_box};

fn field(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(super::field::view(state))
}

fn state() -> ShellState {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.canvas_search.snapshot = Snapshot {
        version: 1,
        session: 7,
        active: true,
        result_count: 1,
        field: Bounds {
            width: 420.0,
            height: 54.0,
            ..Default::default()
        },
        ..Default::default()
    };
    state
}

#[test]
fn typing_keeps_native_editor_focus_and_ignores_older_snapshot_text() {
    let mut state = state();
    let (edits, changes) = tokio::sync::watch::channel(None);
    state.canvas_search.edits = Some(edits);
    let mut ui = Ui::new(state, field, (420, 54));
    let editor = ui.state.canvas_search.target.lock().unwrap().unwrap();
    assert_eq!(ui.harness.focused_widget_id(), Some(editor));
    assert_eq!(
        ui.harness.access_node(editor).unwrap().role(),
        Role::SearchInput
    );
    ui.harness.keyboard_type_chars("Terminal");
    ui.dispatch(&field);
    assert_eq!(ui.state.canvas_search.query, "Terminal");
    assert_eq!(changes.borrow().as_ref().unwrap().query, "Terminal");
    let mut snapshot = ui.state.canvas_search.snapshot.clone();
    snapshot.revision += 1;
    apply_snapshot(&mut ui.state, snapshot.clone());
    ui.rebuild(&field);
    assert_eq!(
        ui.harness.access_node(editor).unwrap().value().as_deref(),
        Some("Terminal")
    );
    snapshot.query = "Terminal".into();
    apply_snapshot(&mut ui.state, snapshot);
    ui.rebuild(&field);
    assert!(ui.state.canvas_search.pending_edit.is_none());
    assert_eq!(ui.harness.focused_widget_id(), Some(editor));
}

#[test]
fn pasted_queries_keep_editor_and_wire_text_within_the_utf8_byte_limit() {
    let limit = hearthspace_common::canvas_search::MAX_QUERY_BYTES;
    let mut state = state();
    let (edits, changes) = tokio::sync::watch::channel(None);
    state.canvas_search.edits = Some(edits);
    let mut ui = Ui::new(state, field, (420, 54));
    let editor = ui.harness.focused_widget_id().unwrap();
    let prefix = "é".repeat(limit / 2 - 1);
    ui.harness
        .process_text_event(TextEvent::ClipboardPaste(format!("{prefix}🖥suffix")));
    ui.dispatch(&field);
    assert_eq!(ui.state.canvas_search.query, prefix);
    assert_eq!(changes.borrow().as_ref().unwrap().query, prefix);
    assert_eq!(
        ui.harness.access_node(editor).unwrap().value().as_deref(),
        Some(prefix.as_str())
    );

    // Even when the bounded draft is unchanged, rebuild must remove newly
    // pasted overflow from the native editor rather than displaying stale text.
    let full = "x".repeat(limit);
    ui.state.canvas_search.query = full.clone();
    ui.rebuild(&field);
    let TextEvent::Keyboard(mut end) = TextEvent::key_down(Key::Named(NamedKey::End)) else {
        unreachable!()
    };
    end.modifiers = Modifiers::CONTROL;
    ui.harness.process_text_event(TextEvent::Keyboard(end));
    ui.harness
        .process_text_event(TextEvent::ClipboardPaste("extra".into()));
    ui.dispatch(&field);
    assert_eq!(ui.state.canvas_search.query, full);
    assert_eq!(changes.borrow().as_ref().unwrap().query, full);
    assert_eq!(
        ui.harness.access_node(editor).unwrap().value().as_deref(),
        Some(full.as_str())
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(editor));
}

#[test]
fn keyboard_navigation_activation_and_escape_use_the_active_session() {
    let mut state = state();
    let (sender, mut commands) = tokio::sync::mpsc::channel(8);
    state.canvas_search.requests = Some(sender);
    let mut ui = Ui::new(state, field, (420, 54));
    for (key, command) in [
        (NamedKey::ArrowDown, "canvas-search-step"),
        (NamedKey::Enter, "canvas-search-activate"),
    ] {
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(key)));
        ui.dispatch(&field);
        let received = commands.try_recv().unwrap();
        let (name, payload) = received.split_once(' ').unwrap();
        assert_eq!(name, command);
        let payload: serde_json::Value = serde_json::from_str(payload).unwrap();
        assert_eq!(payload["session"], 7);
    }
    let close = ui.button("Close canvas search");
    ui.harness.focus_on(Some(close));
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));
    ui.dispatch(&field);
    assert_eq!(commands.try_recv().unwrap(), "canvas-search-close 7");
}

#[test]
fn empty_results_reverse_without_moving_the_field_outside_its_surface() {
    for above in [false, true] {
        let mut state = state();
        state.canvas_search.snapshot.result_count = 0;
        state.canvas_search.snapshot.results_above = above;
        state.canvas_search.snapshot.field.height = 96.0;
        let ui = Ui::new(state, field, (420, 96));
        let close = ui
            .harness
            .access_node(ui.button("Close canvas search"))
            .unwrap()
            .bounding_box()
            .unwrap();
        assert!(close.y0 >= if above { 42.0 } else { 0.0 });
        assert!(close.y1 <= if above { 96.0 } else { 54.0 });
    }
}

#[test]
fn selection_announces_the_result_without_taking_editor_focus() {
    let mut state = state();
    state.canvas_search.snapshot.results = vec![ResultEntry {
        id: "app:terminal.desktop".into(),
        title: "Terminal".into(),
        kind: "item".into(),
        selected: false,
    }];
    let mut ui = Ui::new(state, field, (420, 54));
    let editor = ui.harness.focused_widget_id();
    ui.state.canvas_search.snapshot.selected_id = Some("app:terminal.desktop".into());
    ui.rebuild(&field);
    ui.harness.redraw();
    let announcement = crate::ui::test_support::ids(&ui.harness)
        .into_iter()
        .find_map(|id| {
            ui.harness.access_node(id).filter(|node| {
                node.role() == Role::Status && node.label().as_deref() == Some("Terminal, 1 of 1")
            })
        })
        .expect("selected result has a native announcement");
    assert_eq!(announcement.live(), masonry::accesskit::Live::Polite);
    assert_eq!(ui.harness.focused_widget_id(), editor);
}
