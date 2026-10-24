use super::*;

fn output(name: &str, size: (i32, i32)) -> OutputInfo {
    OutputInfo {
        global_name: 1,
        name: Some(name.into()),
        description: None,
        position: None,
        size: Some(size),
        refresh_millihertz: Some(60_000),
        scale: 1,
    }
}

#[test]
fn mode_changes_reordering_and_hotplug_keep_shell_connector_instances() {
    let mut state = ShellState::new(
        "/unused".into(),
        output_names(vec![
            output("eDP-1", (1920, 1080)),
            output("DP-1", (1920, 1080)),
        ]),
    );
    state.outputs[0].query = "term".into();
    let built_in_ids = state.outputs[0].corner_window_ids;
    let external_ids = state.outputs[1].corner_window_ids;
    state.apply_output_names(output_names(vec![
        output("DP-1", (2560, 1440)),
        output("eDP-1", (1920, 1080)),
        output("DP-3", (1280, 720)),
    ]));
    assert_eq!(state.outputs[0].corner_window_ids, external_ids);
    assert_eq!(state.outputs[1].corner_window_ids, built_in_ids);
    assert_eq!(state.outputs[1].query, "term");
    state.apply_output_names(output_names(vec![
        output("eDP-1", (1920, 1080)),
        output("DP-3", (1280, 720)),
    ]));
    assert_eq!(state.outputs[0].corner_window_ids, built_in_ids);
    assert_eq!(state.outputs[0].query, "term");
}
