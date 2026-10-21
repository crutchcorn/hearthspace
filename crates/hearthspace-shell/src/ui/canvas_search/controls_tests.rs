use super::*;
use crate::ui::test_support::Ui;
use xilem::view::sized_box;

#[test]
fn previews_use_live_status_and_open_the_matching_output_dropdown() {
    let mut state = ShellState::new("/unused".into(), vec!["left".into(), "right".into()]);
    state.clock = "October 21, 12:34".into();
    let clock = ENTRIES
        .iter()
        .find(|entry| entry.id == "control:clock")
        .unwrap();
    assert_eq!(
        detail(&state, clock).1.as_deref(),
        Some("October 21, 12:34")
    );
    let volume = ENTRIES
        .iter()
        .find(|entry| entry.id == "control:volume")
        .unwrap();
    assert_eq!(detail(&state, volume).1.as_deref(), Some("Unavailable"));
    activate(&mut state, "control:brightness", Some("right"));
    assert!(!state.outputs[0].show_controls);
    assert!(state.outputs[1].show_controls);
    assert_eq!(state.outputs[1].quick_panel, Kind::Brightness);
    activate(&mut state, "control:notifications", Some("left"));
    assert!(state.outputs[0].show_notifications);
    assert!(!state.outputs[1].show_controls);
}

#[test]
fn control_card_has_a_native_action_and_does_not_open_until_accepted() {
    fn card(
        state: &ShellState,
    ) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
        sized_box(view(state, &ENTRIES[0]))
    }
    let mut state = ShellState::new("/unused".into(), vec!["left".into()]);
    state.canvas_search.snapshot.session = 3;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
    state.canvas_search.requests = Some(sender);
    let mut ui = Ui::new(state, card, (240, 72));
    ui.activate(ui.button("Notifications All clear"));
    ui.dispatch(&card);
    let command = receiver.try_recv().unwrap();
    assert!(command.starts_with("canvas-search-activate "));
    assert!(command.contains("control:notifications"));
    assert!(!ui.state.outputs[0].show_notifications);
}

#[test]
fn registration_has_only_implemented_controls_and_unique_surface_ids() {
    let ids = window_ids();
    assert_eq!(ids.len(), ENTRIES.len());
    assert!(
        registration()
            .iter()
            .all(|entry| ids.contains_key(entry["id"].as_str().unwrap()))
    );
    assert!(
        !registration()
            .iter()
            .any(|entry| entry["id"] == "tool:draw" || entry["id"] == "tool:layout")
    );
}
