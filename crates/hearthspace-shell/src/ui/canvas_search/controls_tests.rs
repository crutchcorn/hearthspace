use super::*;
use crate::ui::test_support::Ui;
use xilem::view::sized_box;

#[test]
fn accepted_pan_action_requires_the_frame_worker_and_does_not_select_optimistically() {
    let mut state = ShellState::new("/unused".into(), vec!["test".into()]);
    state.frame_layout.snapshot.canvas_tool = Some(CanvasTool::Search);
    state.outputs[0].show_map = true;
    activate(&mut state, "tool:pan", Some("test"));
    assert!(!state.outputs[0].show_map);
    assert_eq!(
        state.frame_layout.snapshot.canvas_tool,
        Some(CanvasTool::Search)
    );
    assert_eq!(
        state.frame_layout.error.as_deref(),
        Some("Canvas tools are not ready. Try again shortly.")
    );
    assert!(state.canvas_search.error.is_none());
}

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
    state.canvas_search.snapshot.active = true;
    state
        .canvas_search
        .snapshot
        .results
        .push(crate::ui::canvas_search::ResultEntry {
            id: "control:notifications".into(),
            title: "Notifications".into(),
            kind: "control".into(),
            selected: false,
        });
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
    state.canvas_search.requests = Some(sender);
    let mut ui = Ui::new(state, card, (240, 72));
    let button = ui.button("Notifications All clear");
    ui.activate(button);
    ui.dispatch(&card);
    let command = receiver.try_recv().unwrap();
    assert!(command.starts_with("canvas-search-activate "));
    assert!(command.contains("control:notifications"));
    assert!(command.contains("\"session\":3"));
    assert!(!ui.state.outputs[0].show_notifications);

    let mut reopened = ui.state.canvas_search.snapshot.clone();
    super::super::apply_snapshot(&mut ui.state, Default::default());
    ui.rebuild(&card);
    ui.harness.redraw();
    assert!(
        ui.harness
            .access_node(button)
            .is_none_or(|node| node.is_hidden())
    );
    ui.activate(button);
    ui.dispatch(&card);
    assert!(receiver.try_recv().is_err());

    reopened.session = 4;
    super::super::apply_snapshot(&mut ui.state, reopened);
    ui.rebuild(&card);
    ui.harness.redraw();
    assert_eq!(ui.button("Notifications All clear"), button);
    assert!(!ui.harness.access_node(button).unwrap().is_disabled());
    ui.activate(button);
    ui.dispatch(&card);
    assert!(receiver.try_recv().unwrap().contains("\"session\":4"));
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
    let pan = registration()
        .into_iter()
        .find(|entry| entry["id"] == "tool:pan")
        .unwrap();
    assert_eq!(pan["title"], "Pan canvas");
    for keyword in ["hand", "grab", "drag", "navigate"] {
        assert!(pan["search_text"].as_str().unwrap().contains(keyword));
    }
}
