use super::*;
use crate::{
    actions::{MonitorEditorAction, apply_monitor_editor_action},
    state::SettingsCategory,
    test_support::Ui,
};
use hearthspace_ipc::Settings;
use xilem::{WidgetView, view::sized_box};

fn output(id: &str) -> LiveOutput {
    LiveOutput {
        id: id.into(),
        name: id.into(),
        width: 1920,
        height: 1080,
        scale: 1,
        refresh_millihertz: Some(60_000),
    }
}

fn snapshot(outputs: Vec<LiveOutput>) -> Snapshot {
    Snapshot {
        outputs,
        primary: Some("eDP-1".into()),
    }
}

fn state() -> (tempfile::TempDir, SettingsApp) {
    let (directory, favorites) = crate::favorites::tests::fixture();
    let settings = Settings::default();
    let state = SettingsApp {
        running: true,
        navigation: Default::default(),
        preferences: Default::default(),
        appearance_theme: Default::default(),
        activation_listener: None,
        ai: Default::default(),
        favorites,
        frame_settings: Default::default(),
        display_layout: Default::default(),
        saved_monitors: settings.monitors.clone(),
        settings,
        output_observation: Default::default(),
        live_outputs: vec![],
        selected_category: SettingsCategory::Displays,
        selected_monitor_id: None,
        background_preview: Default::default(),
        background_picker_request_sender: None,
        background_picker_pending: false,
        status: String::new(),
    };
    (directory, state)
}

fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(crate::display_view::view(state))
}

#[test]
fn reordered_observations_keep_selected_primary_roles_connector_identity_and_selection() {
    let (_directory, mut state) = state();
    state.saved_monitors[0].scale = 2;
    state.saved_monitors[1].x = 960;
    let saved = state.saved_monitors.clone();
    apply_snapshot(&mut state, snapshot(vec![output("DP-1"), output("eDP-1")]));
    assert_eq!(state.settings.monitors[0].id, "eDP-1");
    assert_eq!(state.settings.monitors[0].scale, 2);
    assert_eq!(state.settings.monitors[1].id, "DP-1");
    assert_eq!(state.settings.monitors[1].x, 960);
    state.selected_monitor_id = Some("DP-1".into());
    let before = state.settings.monitors.clone();
    apply_snapshot(&mut state, snapshot(vec![output("eDP-1"), output("DP-1")]));
    assert_eq!(state.settings.monitors, before);
    assert_eq!(state.selected_monitor_id.as_deref(), Some("DP-1"));
    assert_eq!(state.saved_monitors, saved);
}

#[test]
fn open_displays_page_updates_mode_frequency_and_selection_after_hotplug() {
    let (_directory, mut state) = state();
    apply_snapshot(&mut state, snapshot(vec![output("eDP-1")]));
    let mut ui = Ui::new(state, view, (470, 490));
    assert!(ui.harness.access_node(ui.button("60 Hz")).is_some());
    let mut dock = output("DP-1");
    dock.width = 2560;
    dock.height = 1440;
    dock.refresh_millihertz = Some(75_000);
    apply_snapshot(&mut ui.state, snapshot(vec![dock.clone(), output("eDP-1")]));
    ui.state.selected_monitor_id = Some("DP-1".into());
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(ui.harness.access_node(ui.button("75 Hz")).is_some());
    assert_eq!(ui.state.settings.monitors[1].width, 2560);
    dock.width = 3840;
    dock.height = 2160;
    dock.refresh_millihertz = Some(59_940);
    apply_snapshot(&mut ui.state, snapshot(vec![output("eDP-1"), dock]));
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(ui.harness.access_node(ui.button("59.94 Hz")).is_some());
    assert_eq!(ui.state.settings.monitors[1].width, 3840);
    apply_snapshot(&mut ui.state, snapshot(vec![output("eDP-1")]));
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(ui.state.selected_monitor_id.as_deref(), Some("eDP-1"));
    assert_eq!(ui.state.settings.monitors.len(), 1);
    assert!(ui.harness.access_node(ui.button("60 Hz")).is_some());
}

#[test]
fn pending_drag_survives_mode_changes_removal_and_replug_without_mutating_preferences() {
    let (_directory, mut state) = state();
    apply_snapshot(&mut state, snapshot(vec![output("eDP-1"), output("DP-1")]));
    let saved = state.saved_monitors.clone();
    let mut preview = state.settings.monitors.clone();
    preview[0].scale = 2;
    preview[1].x = 960;
    preview[1].y = 120;
    preview[1].scale = 2;
    apply_monitor_editor_action(
        &mut state,
        MonitorEditorAction::Drag {
            moved_id: "DP-1".into(),
            monitors: preview,
            commit: false,
        },
    );
    let mut changed = output("DP-1");
    changed.width = 2880;
    apply_snapshot(
        &mut state,
        snapshot(vec![changed.clone(), output("eDP-1"), output("DP-3")]),
    );
    let draft = state
        .settings
        .monitors
        .iter()
        .find(|row| row.id == "DP-1")
        .unwrap();
    assert_eq!(
        (draft.width, draft.x, draft.y, draft.scale),
        (2880, 960, 120, 2)
    );
    assert_eq!(state.selected_monitor_id.as_deref(), Some("DP-1"));
    apply_snapshot(&mut state, snapshot(vec![output("eDP-1")]));
    assert_eq!(state.selected_monitor_id.as_deref(), Some("eDP-1"));
    apply_snapshot(&mut state, snapshot(vec![changed, output("eDP-1")]));
    let draft = state
        .settings
        .monitors
        .iter()
        .find(|row| row.id == "DP-1")
        .unwrap();
    assert_eq!(
        (draft.width, draft.x, draft.y, draft.scale),
        (2880, 960, 120, 2)
    );
    assert!(state.display_layout.is_pending());
    assert_eq!(state.saved_monitors, saved);
}

#[test]
fn removing_all_outputs_keeps_disconnected_preferences_and_clears_visible_rows() {
    let (_directory, mut state) = state();
    state.saved_monitors[1].id = "DP-1".into();
    state.saved_monitors[1].x = 2400;
    let saved = state.saved_monitors.clone();
    apply_snapshot(&mut state, snapshot(vec![output("DP-1")]));
    apply_snapshot(&mut state, snapshot(vec![]));
    assert!(state.settings.monitors.is_empty());
    assert_eq!(state.selected_monitor_id, None);
    assert_eq!(state.saved_monitors, saved);
    apply_snapshot(&mut state, snapshot(vec![output("DP-1")]));
    assert_eq!(state.settings.monitors[0].x, 2400);
    assert_eq!(state.selected_monitor_id.as_deref(), Some("DP-1"));
}

#[test]
fn unavailable_primary_metadata_does_not_assign_roles_using_registry_order() {
    let (_directory, mut state) = state();
    state.saved_monitors[0].scale = 3;
    apply_snapshot(
        &mut state,
        Snapshot {
            outputs: vec![output("eDP-1"), output("DP-1")],
            primary: None,
        },
    );
    assert!(state.settings.monitors.iter().all(|row| row.scale == 1));
    assert_eq!(state.settings.monitors[0].id, "DP-1");
    assert_eq!(state.saved_monitors[0].scale, 3);
}
