use super::*;

#[test]
fn output_targets_fall_back_to_primary_when_no_outputs_are_known() {
    assert_eq!(normalized_output_targets(Vec::new()), vec![None]);
}

#[test]
fn output_targets_keep_order_and_drop_duplicates() {
    assert_eq!(
        normalized_output_targets(vec![
            "eDP-1".to_string(),
            "HDMI-A-1".to_string(),
            "eDP-1".to_string(),
            String::new(),
        ]),
        vec![Some("eDP-1".to_string()), Some("HDMI-A-1".to_string())]
    );
}

#[test]
fn shell_state_preserves_window_ids_and_query_across_output_updates() {
    let mut state = ShellState::new(PathBuf::from("/tmp/socket"), vec!["eDP-1".to_string()]);
    state.outputs[0].query = "term".to_string();
    let corner_window_ids = state.outputs[0].corner_window_ids;

    state.apply_output_names(vec!["eDP-1".to_string(), "HDMI-A-1".to_string()]);

    assert_eq!(state.outputs[0].corner_window_ids, corner_window_ids);
    assert_eq!(state.outputs[0].query, "term");
    assert_eq!(state.outputs[1].output_name.as_deref(), Some("HDMI-A-1"));
}

#[test]
fn rejected_launch_preserves_query_and_shows_error_until_retry_or_edit() {
    let mut state = ShellState::new(PathBuf::from("/tmp/socket"), vec!["eDP-1".into()]);
    state.change_query("eDP-1", "editor".into());
    state.finish_launch("eDP-1", Err("missing executable".into()));
    assert_eq!(state.outputs[0].query, "editor");
    assert_eq!(
        state.outputs[0].launch_error.as_deref(),
        Some("missing executable")
    );
    state.apply_output_names(vec!["eDP-1".into()]);
    assert!(state.outputs[0].launch_error.is_some());
    state.change_query("eDP-1", "terminal".into());
    assert!(state.outputs[0].launch_error.is_none());
    state.finish_launch("eDP-1", Ok(()));
    assert!(state.outputs[0].query.is_empty());
}

#[test]
fn map_toggle_is_exclusive_and_other_shell_panels_dismiss_it() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into(), "HDMI-A-1".into()]);
    state.outputs[0].show_notifications = true;
    state.outputs[0].show_search = true;
    state.outputs[0].query = "notes".into();
    state.toggle_map("eDP-1");
    assert!(state.outputs[0].show_map);
    assert!(!state.outputs[0].show_notifications);
    assert!(!state.outputs[0].show_search);
    assert!(state.outputs[0].query.is_empty());
    state.toggle_map("HDMI-A-1");
    assert!(!state.outputs[0].show_map);
    assert!(state.outputs[1].show_map);
    state.toggle_map("HDMI-A-1");
    assert!(!state.outputs[1].show_map);
    state.toggle_map("eDP-1");
    state.change_query("eDP-1", "editor".into());
    assert!(!state.outputs[0].show_map);
    state.toggle_map("eDP-1");
    state.toggle_search("eDP-1");
    assert!(!state.outputs[0].show_map);
    state.toggle_map("eDP-1");
    state.dismiss_launcher("eDP-1");
    assert!(!state.outputs[0].show_map);
}

#[test]
fn only_one_panel_can_be_open_across_outputs_and_stale_closes_are_scoped() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into(), "HDMI-A-1".into()]);
    state.toggle_search("eDP-1");
    state.change_query("eDP-1", "Settings".into());
    state.toggle_notifications("HDMI-A-1");
    assert!(!state.outputs[0].show_search);
    assert!(state.outputs[0].query.is_empty());
    assert!(state.outputs[1].show_notifications);
    state.dismiss_search("HDMI-A-1");
    assert!(state.outputs[1].show_notifications);
    state.toggle_power("HDMI-A-1");
    state.outputs[1].confirm_logout = true;
    state.outputs[1].launch_error = Some("retry".into());
    state.dismiss_notifications("HDMI-A-1");
    state.dismiss_search("HDMI-A-1");
    state.dismiss_map("HDMI-A-1");
    assert!(state.outputs[1].show_power);
    assert!(state.outputs[1].confirm_logout);
    assert!(state.outputs[1].launch_error.is_some());
    state.toggle_search("HDMI-A-1");
    state.change_query("HDMI-A-1", "Notes".into());
    state.dismiss_power("HDMI-A-1");
    assert!(state.outputs[1].show_search);
    assert_eq!(state.outputs[1].query, "Notes");
    assert!(!state.outputs[1].confirm_logout);
}
