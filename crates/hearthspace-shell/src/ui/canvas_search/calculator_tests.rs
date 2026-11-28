//! Calculator activation uses the same native search field and card buttons.
use super::super::{ResultEntry, Snapshot, apply_snapshot};
use super::*;
use crate::ui::test_support::Ui;
use masonry::core::{
    TextEvent,
    keyboard::{Key, NamedKey},
};
use xilem::{WidgetView, view::sized_box};

fn state() -> (ShellState, tokio::sync::mpsc::Receiver<String>) {
    let mut state = ShellState::new("/unused".into(), vec!["test".into()]);
    let search = &mut state.canvas_search;
    search.query = "6 * 7".into();
    search.snapshot = Snapshot {
        session: 3,
        active: true,
        query: search.query.clone(),
        result_count: 1,
        selected_id: Some(CALCULATION.id.into()),
        results: vec![ResultEntry {
            id: CALCULATION.id.into(),
            title: "42".into(),
            kind: "calculation".into(),
            selected: true,
        }],
        ..Default::default()
    };
    let (sender, receiver) = tokio::sync::mpsc::channel(4);
    search.requests = Some(sender);
    (state, receiver)
}

fn activation(command: String) -> serde_json::Value {
    let payload = command.strip_prefix("canvas-search-activate ").unwrap();
    serde_json::from_str(payload).unwrap()
}

#[test]
fn enter_in_the_native_field_sends_the_current_query_with_the_selected_answer() {
    fn field(
        state: &ShellState,
    ) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
        sized_box(super::super::field::view(state))
    }
    let (state, mut requests) = state();
    let mut ui = Ui::new(state, field, (420, 54));
    let editor = ui.harness.focused_widget_id().unwrap();
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
    ui.dispatch(&field);
    let first = activation(requests.try_recv().unwrap());
    assert_eq!(
        first,
        serde_json::json!({"session":3,"id":null,"query":"6 * 7"})
    );

    // The pending draft must not activate the old answer if it overtakes an edit.
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::End)));
    ui.harness.keyboard_type_chars(" + 1");
    ui.dispatch(&field);
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
    ui.dispatch(&field);
    assert_eq!(
        activation(requests.try_recv().unwrap())["query"],
        "6 * 7 + 1"
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(editor));
}

#[test]
fn calculator_card_copies_on_mouse_activation_and_is_inaccessible_after_dismissal() {
    fn card(
        state: &ShellState,
    ) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
        sized_box(super::view(state, &CALCULATION))
    }
    let (state, mut requests) = state();
    let mut ui = Ui::new(state, card, (240, 72));
    let button = ui.button("= 42 Copy answer");
    ui.harness.mouse_click_on(button, None);
    ui.dispatch(&card);
    assert_eq!(
        activation(requests.try_recv().unwrap()),
        serde_json::json!({
            "session":3,"id":CALCULATION.id,"query":"6 * 7"
        })
    );
    apply_snapshot(&mut ui.state, Snapshot::default());
    ui.rebuild(&card);
    ui.harness.redraw();
    assert!(
        ui.harness
            .access_node(button)
            .is_none_or(|node| node.is_hidden())
    );
    ui.activate(button);
    ui.dispatch(&card);
    assert!(requests.try_recv().is_err());
}
