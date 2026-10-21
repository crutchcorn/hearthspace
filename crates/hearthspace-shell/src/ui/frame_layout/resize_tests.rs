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
