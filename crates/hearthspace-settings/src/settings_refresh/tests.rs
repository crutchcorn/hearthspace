use super::*;
use crate::{
    actions::{MonitorEditorAction, apply_monitor_editor_action},
    monitors::LiveOutput,
    state::SettingsCategory,
    test_support::Ui,
    wayland_outputs::{Snapshot, apply_snapshot},
};
use hearthspace_ipc::{AccentColor, ColorMode, Settings};
use xilem::{WidgetView, view::sized_box};

fn state() -> (tempfile::TempDir, SettingsApp) {
    let (directory, favorites) = crate::favorites::tests::fixture();
    let state = SettingsApp {
        running: true,
        preferences: Default::default(),
        navigation: Default::default(),
        appearance_theme: Default::default(),
        activation_listener: None,
        ai: Default::default(),
        favorites,
        frame_settings: Default::default(),
        display_layout: Default::default(),
        saved_monitors: Settings::default().monitors,
        settings: Settings::default(),
        output_observation: Default::default(),
        live_outputs: vec![],
        selected_category: SettingsCategory::Appearance,
        selected_monitor_id: None,
        background_preview: Default::default(),
        background_picker_request_sender: None,
        background_picker_pending: false,
        status: String::new(),
    };
    (directory, state)
}

fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(crate::appearance_view::view(state))
}
fn live(state: &mut SettingsApp) {
    apply_snapshot(
        state,
        Snapshot {
            outputs: vec![LiveOutput {
                id: "DP-1".into(),
                name: "DP-1".into(),
                width: 2560,
                height: 1440,
                scale: 1,
                refresh_millihertz: Some(60_000),
            }],
            primary: Some("DP-1".into()),
        },
    );
}

#[test]
fn external_commit_refreshes_rendered_appearance_and_saved_preferences_while_drag_stays_local() {
    let (_directory, mut state) = state();
    live(&mut state);
    let mut preview = state.settings.monitors.clone();
    preview[0].x = 500;
    apply_monitor_editor_action(
        &mut state,
        MonitorEditorAction::Drag {
            moved_id: "DP-1".into(),
            monitors: preview.clone(),
            commit: false,
        },
    );
    let mut ui = Ui::new(state, view, (470, 440));
    let dark = ui.button("Dark");
    let mut external = Settings::default();
    external.appearance.color_mode = ColorMode::Dark;
    external.appearance.accent_color = AccentColor::Blue;
    external.monitors[0].scale = 2;
    external.monitors[1].x = 960;
    let saved = external.monitors.clone();
    apply_refresh(
        &mut ui.state,
        (0, SettingsSnapshot::new(4, external).unwrap()),
    );
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(ui.button("Dark"), dark);
    assert_eq!(
        ui.harness.access_node(dark).unwrap().toggled(),
        Some(xilem::masonry::accesskit::Toggled::True)
    );
    assert_eq!(ui.state.settings.monitors, preview);
    assert_eq!(ui.state.saved_monitors, saved);
    assert_eq!(ui.state.selected_monitor_id.as_deref(), Some("DP-1"));
    assert!(ui.state.display_layout.is_pending());
}

#[test]
fn external_display_preferences_use_current_connected_sizes_and_keep_disconnected_rows() {
    let (_directory, mut state) = state();
    live(&mut state);
    let mut external = Settings::default();
    external.monitors[0].scale = 2;
    external.monitors[1].x = 960;
    let saved = external.monitors.clone();
    apply_refresh(&mut state, (0, SettingsSnapshot::new(8, external).unwrap()));
    assert_eq!(state.settings.monitors.len(), 1);
    assert_eq!(state.settings.monitors[0].id, "DP-1");
    assert_eq!(state.settings.monitors[0].width, 2560);
    assert_eq!(state.settings.monitors[0].scale, 2);
    assert_eq!(state.saved_monitors, saved);
}

#[test]
fn queued_snapshot_cannot_overwrite_local_ack_and_a_pending_picker_owns_its_path() {
    let (_directory, mut state) = state();
    state.settings.appearance.color_mode = ColorMode::Dark;
    local_save_finished(&mut state);
    apply_refresh(
        &mut state,
        (0, SettingsSnapshot::new(1, Settings::default()).unwrap()),
    );
    assert_eq!(state.settings.appearance.color_mode, ColorMode::Dark);
    state.background_picker_pending = true;
    state.settings.background_image_path = Some("local draft.png".into());
    let external = Settings {
        background_image_path: Some("external.png".into()),
        test: true,
        ..Settings::default()
    };
    apply_refresh(&mut state, (1, SettingsSnapshot::new(2, external).unwrap()));
    assert_eq!(
        state.settings.background_image_path.as_deref(),
        Some("local draft.png")
    );
    assert!(state.settings.test);
}

mod picker;
