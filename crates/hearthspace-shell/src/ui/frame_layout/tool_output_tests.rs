use super::*;

#[test]
fn removing_one_output_menu_does_not_clear_another_outputs_focus_targets() {
    let (mut state, _requests) = fixture(CanvasTool::Pan);
    state.apply_output_names(vec!["test".into(), "other".into()]);
    state.outputs[0].show_controls = true;
    let build = |state: &ShellState| {
        sized_box(xilem::view::flex_row((
            state.outputs[0]
                .show_controls
                .then(|| tool_menu::panel(state, &state.outputs[0])),
            tool_menu::panel(state, &state.outputs[1]),
        )))
    };
    let mut ui = Ui::new(state, build, (520, 144));
    let first_pan = ui.state.outputs[0].tool_menu_targets[1]
        .lock()
        .unwrap()
        .unwrap();
    let second_pan = ui.state.outputs[1].tool_menu_targets[1]
        .lock()
        .unwrap()
        .unwrap();
    assert_ne!(first_pan, second_pan);
    ui.state.outputs[0].show_controls = false;
    ui.rebuild(&build);
    assert_eq!(
        *ui.state.outputs[0].tool_menu_targets[1].lock().unwrap(),
        None
    );
    assert_eq!(
        *ui.state.outputs[1].tool_menu_targets[1].lock().unwrap(),
        Some(second_pan)
    );
    ui.harness.focus_on(Some(second_pan));
    key(&mut ui, Key::Named(NamedKey::ArrowDown), &build);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Search canvas"))
    );
    let target = ui.state.outputs[1].tool_menu_targets[1].clone();
    ui.state
        .apply_output_names(vec!["other".into(), "test".into()]);
    assert!(std::sync::Arc::ptr_eq(
        &target,
        &ui.state.outputs[0].tool_menu_targets[1]
    ));
}

#[test]
fn confirmed_tool_and_remembered_primary_are_shared_across_output_reordering() {
    let (mut state, _requests) = fixture(CanvasTool::Pan);
    state.apply_output_names(vec!["test".into(), "other".into()]);
    let mut snapshot = state.frame_layout.snapshot.clone();
    snapshot.outputs.push(Output {
        output: "other".into(),
        compact: true,
        ..Default::default()
    });
    apply(&mut state, snapshot);
    let two_outputs = |state: &ShellState| {
        sized_box(xilem::view::flex_row((
            widgets::content(
                state,
                &state.outputs[0],
                "tools",
                false,
                false,
                Default::default(),
                &state.frame_layout.chains[&widget_key("test", "tools")],
            ),
            widgets::content(
                state,
                &state.outputs[1],
                "tools",
                false,
                false,
                Default::default(),
                &state.frame_layout.chains[&widget_key("other", "tools")],
            ),
        )))
    };
    let mut ui = Ui::new(state, two_outputs, (232, 44));
    assert_eq!(toggled(&ui, PAN_TRIGGER), Some(Toggled::True));
    assert_eq!(toggled(&ui, "Canvas tools"), Some(Toggled::True));
    observe(&mut ui.state, Some(CanvasTool::Layout));
    ui.rebuild(&two_outputs);
    ui.dispatch(&two_outputs);
    assert_eq!(toggled(&ui, PAN_TRIGGER), Some(Toggled::False));
    assert_eq!(toggled(&ui, "Change layout"), Some(Toggled::True));
    ui.state
        .apply_output_names(vec!["other".into(), "test".into()]);
    assert_eq!(tool_menu::primary(&ui.state), CanvasTool::Pan);
    observe(&mut ui.state, None);
    assert_eq!(tool_menu::primary(&ui.state), CanvasTool::Pan);
    assert_eq!(ui.state.frame_layout.snapshot.canvas_tool, None);
}
