use super::*;

fn item(id: &str) -> DesktopItem {
    DesktopItem {
        id: format!("app:{id}.desktop"),
        title: id.into(),
        kind: DesktopItemKind::App {
            app_id: format!("{id}.desktop"),
            icon: None,
        },
    }
}

#[test]
fn catalog_changes_preserve_surfaces_for_existing_items() {
    let mut state = State::default();
    state.replace_items(vec![item("editor"), item("files")]);
    let editor = state.items[0].window_id;
    let files = state.items[1].window_id;
    let mut renamed = item("editor");
    renamed.title = "New editor label".into();
    state.replace_items(vec![item("settings"), item("files"), renamed]);
    assert_eq!(state.items[1].window_id, files);
    assert_eq!(state.items[2].window_id, editor);
    assert_eq!(state.items[2].content.title, "New editor label");
    assert_ne!(state.items[0].window_id, files);
    state.replace_items(vec![item("editor")]);
    assert_eq!(state.items.len(), 1);
    assert_eq!(state.items[0].window_id, editor);
}

#[test]
fn desktop_snapshot_carries_selection_and_keyboard_focus_without_positions() {
    let snapshot = events::parse_snapshot(br#"{"version":1,"revision":4,"select_mode":true,"items":[{"id":"app:editor.desktop","x":20,"y":40,"selected":true,"focused":true,"keyboard_focus":false}]}"#).unwrap();
    assert_eq!(snapshot.revision, 4);
    assert!(snapshot.select_mode);
    let item = &snapshot.items["app:editor.desktop"];
    assert!(item.selected && item.focused && !item.keyboard_focus);
    assert!(
        events::parse_snapshot(br#"{"version":2,"revision":4,"select_mode":true,"items":[]}"#)
            .is_err()
    );
    assert!(events::parse_snapshot(br#"{"version":1,"revision":4,"select_mode":true,"items":[{"id":"same","selected":false,"focused":false,"keyboard_focus":false},{"id":"same","selected":true,"focused":false,"keyboard_focus":false}]}"#).is_err());
}

#[test]
fn desktop_events_require_version_and_nonzero_authorization_tokens() {
    use serde_json::json;
    assert_eq!(
        parse_event(&json!({"version":1,"action":"desktop-changed"})),
        Some(Event::Changed)
    );
    assert_eq!(
        parse_event(&json!({"version":1,"action":"desktop-action","token":7})),
        Some(Event::Action(7))
    );
    assert_eq!(
        parse_event(&json!({"version":1,"action":"desktop-action","token":0})),
        None
    );
    assert_eq!(
        parse_event(&json!({"version":2,"action":"desktop-action","token":7})),
        None
    );
    assert_eq!(
        parse_event(&json!({"version":1,"action":"desktop-action","id":"app:editor.desktop"})),
        None
    );
}

#[test]
fn queued_action_without_worker_does_not_open_ui_after_session_loss() {
    let mut state = ShellState::new("/unused".into(), vec!["test".into()]);
    apply_event(&mut state, Event::Action(7));
    assert!(!state.outputs[0].show_search);
    assert!(state.desktop.error.is_some());
}
