use super::*;

#[test]
fn inspector_content_shrinks_before_old_window_allocation_is_reconfigured() {
    let mut state = state();
    state.frame_layout.snapshot.selected = Some(Selection {
        output: "test".into(),
        target: LayoutTarget::widget("clock"),
    });
    let mut ui = Ui::new(state, inspector, (260, 452));
    for (target, expected) in [
        (LayoutTarget::widget("clock"), 452.0),
        (LayoutTarget::new(TargetKind::Group, "system"), 156.0),
        (LayoutTarget::new(TargetKind::Item, "file:example"), 103.0),
    ] {
        ui.state
            .frame_layout
            .snapshot
            .selected
            .as_mut()
            .unwrap()
            .target = target;
        ui.rebuild(&inspector);
        ui.harness.redraw();
        assert_eq!(ui.harness.root_widget().ctx().border_box().height(), 452.0);
        let content = ids(&ui.harness)
            .into_iter()
            .filter_map(|id| ui.harness.access_node(id))
            .find(|node| {
                node.role() == Role::Group && node.label().as_deref() == Some("Change layout")
            })
            .unwrap();
        assert_eq!(content.bounding_box().unwrap().height(), expected - 10.0);
    }
}

fn measured_confirmation(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    let height = state
        .frame_layout
        .confirm_height
        .unwrap_or(220.0)
        .min(state.frame_layout.snapshot.confirmation_max_height);
    sized_box(confirm::view(state, 420.0, height))
}

#[test]
fn confirmation_natural_height_hides_rounding_scrollbar_but_preserves_real_overflow() {
    use masonry::core::WindowEvent;
    for scale in [1.0, 2.0] {
        let mut state = state();
        state.frame_layout.snapshot.confirmation = Some(Confirmation::CloseWindow { window_id: 1 });
        let mut ui = Ui::new(state, measured_confirmation, (420, 220));
        let height = ui.state.frame_layout.confirm_height.unwrap();
        ui.harness.process_window_event(WindowEvent::Rescale(scale));
        ui.harness.process_window_event(WindowEvent::Resize(
            ((420.0 * scale) as u32, (height * scale) as u32).into(),
        ));
        ui.dispatch(&measured_confirmation);
        for id in ids(&ui.harness) {
            let widget = ui.harness.get_widget_with_id(id);
            if widget.downcast::<masonry::widgets::ScrollBar>().is_some() {
                assert!(widget.ctx().is_stashed(), "natural dialog must not scroll");
            }
        }
        ui.state.frame_layout.snapshot.confirmation_max_height = 110.0;
        ui.rebuild(&measured_confirmation);
        ui.harness.process_window_event(WindowEvent::Resize(
            ((420.0 * scale) as u32, (110.0 * scale) as u32).into(),
        ));
        ui.dispatch(&measured_confirmation);
        assert!(
            ids(&ui.harness).into_iter().any(|id| {
                let widget = ui.harness.get_widget_with_id(id);
                widget.downcast::<masonry::widgets::ScrollBar>().is_some()
                    && !widget.ctx().is_stashed()
            }),
            "constrained dialog must retain scrolling"
        );
    }
}

#[test]
fn inspector_long_window_title_keeps_header_actions_visible_and_keyboard_usable() {
    use masonry::core::{
        TextEvent,
        keyboard::{Key, NamedKey},
    };
    let title = "Résumé 🖥 — a very long document title with an unbroken suffix ".repeat(8);
    for width in [260, 160, 120] {
        let mut state = state();
        let target = LayoutTarget::new(TargetKind::Window, "7");
        state.frame_layout.snapshot.outputs[0]
            .layout
            .set_placement(&target, ObjectPlacement::frame(FrameEdge::Top, 0.5));
        state.frame_layout.snapshot.selected = Some(Selection {
            output: "test".into(),
            target,
        });
        state.frame_layout.snapshot.selected_title = title.clone();
        state.frame_layout.snapshot.editor_max_width = f64::from(width);
        let (sender, mut requests) = mpsc::channel(8);
        state.frame_layout.requests = Some(sender);
        let mut ui = Ui::new(state, inspector, (width, 103));
        let done = ui.button("Done changing layout");
        for name in ["Remove from frame", "Done changing layout"] {
            let rect = ui
                .harness
                .access_node(ui.button(name))
                .unwrap()
                .bounding_box()
                .unwrap();
            assert!(
                rect.x0 >= 0.0 && rect.x1 <= f64::from(width) && rect.y1 <= 54.0,
                "{name} must fit at {width}px: {rect:?}"
            );
            assert_eq!(rect.width(), 44.0);
        }
        let nodes: Vec<_> = ids(&ui.harness)
            .into_iter()
            .filter_map(|id| ui.harness.access_node(id))
            .collect();
        assert!(
            nodes
                .iter()
                .any(|node| node.label().as_deref() == Some(title.as_str())),
            "the full window title must remain accessible"
        );
        if width >= 160 {
            assert!(
                nodes
                    .iter()
                    .any(|node| node.value().is_some_and(|text| text.ends_with('…'))),
                "an overflowing title must visibly end in an ellipsis"
            );
        }
        ui.harness.focus_on(Some(done));
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
        ui.harness
            .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        ui.dispatch(&inspector);
        let command = requests.try_recv().unwrap().command;
        let payload: serde_json::Value =
            serde_json::from_str(command.strip_prefix("layout-command ").unwrap()).unwrap();
        assert_eq!(payload["action"], "done");
        assert_eq!(payload["session"], 9);
    }
}

#[test]
fn inspector_titles_refit_after_title_and_width_changes_without_losing_focus() {
    let mut state = state();
    let target = LayoutTarget::new(TargetKind::Window, "7");
    state.frame_layout.snapshot.outputs[0]
        .layout
        .set_placement(&target, ObjectPlacement::frame(FrameEdge::Top, 0.5));
    state.frame_layout.snapshot.selected = Some(Selection {
        output: "test".into(),
        target,
    });
    state.frame_layout.snapshot.selected_title = "Notes".into();
    let mut ui = Ui::new(state, inspector, (260, 103));
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness
            .access_node(id)
            .is_some_and(|node| node.value().as_deref() == Some("Notes"))
    }));
    let done = ui.button("Done changing layout");
    ui.harness.focus_on(Some(done));
    ui.state.frame_layout.snapshot.selected_title = "W".repeat(200);
    ui.state.frame_layout.snapshot.editor_max_width = 160.0;
    ui.rebuild(&inspector);
    ui.dispatch(&inspector);
    assert_eq!(ui.harness.focused_widget_id(), Some(done));
    let rect = ui
        .harness
        .access_node(done)
        .unwrap()
        .bounding_box()
        .unwrap();
    assert!(
        rect.x1 <= 160.0,
        "the checkmark must fit before the native buffer shrinks: {rect:?}"
    );
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness
            .access_node(id)
            .is_some_and(|node| node.value().is_some_and(|text| text.ends_with('…')))
    }));
    ui.state.frame_layout.snapshot.selected_title = "Notes".into();
    ui.state.frame_layout.snapshot.editor_max_width = 260.0;
    ui.rebuild(&inspector);
    ui.dispatch(&inspector);
    assert_eq!(ui.harness.focused_widget_id(), Some(done));
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness
            .access_node(id)
            .is_some_and(|node| node.value().as_deref() == Some("Notes"))
    }));
}
