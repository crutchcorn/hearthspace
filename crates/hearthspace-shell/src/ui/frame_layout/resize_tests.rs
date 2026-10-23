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
        state.frame_layout.snapshot.confirmation = Some(Confirmation {
            kind: "close-window".into(),
            window_id: Some(1),
        });
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
