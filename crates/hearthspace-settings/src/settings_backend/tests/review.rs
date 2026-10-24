use super::*;
use crate::{
    monitors::LiveOutput,
    wayland_outputs::{Snapshot, apply_snapshot},
};

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

fn connected(state: &mut SettingsApp, ids: &[&str]) {
    apply_snapshot(
        state,
        Snapshot {
            outputs: ids.iter().map(|id| output(id)).collect(),
            primary: Some("eDP-1".into()),
        },
    );
}

#[test]
fn saving_remaining_screen_does_not_acknowledge_a_disconnected_failed_draft() {
    let (_directory, mut state, mut receiver) = state();
    connected(&mut state, &["eDP-1", "DP-1"]);
    let mut draft = state.settings.monitors.clone();
    draft[1].x = 2100;
    draft[1].y = 80;
    draft[1].scale = 2;
    let disconnected_draft = draft[1].clone();
    apply_monitor_editor_action(
        &mut state,
        MonitorEditorAction::Drag {
            moved_id: "DP-1".into(),
            monitors: draft,
            commit: true,
        },
    );
    let failed = receiver.try_recv().unwrap();
    receive_display_layout(
        &mut state,
        LayoutResponse {
            request: failed,
            saved: Err("offline".into()),
        },
    );
    connected(&mut state, &["eDP-1"]);
    save_monitor_scale(&mut state, "eDP-1", 2);
    let request = receiver.try_recv().unwrap();
    assert_eq!(request.monitors.len(), 1);
    assert_eq!(request.monitors[0].id, "eDP-1");
    receive_display_layout(&mut state, successful(request));
    assert!(
        state.display_layout.is_pending(),
        "DP-1 was never submitted by the successful save"
    );
    connected(&mut state, &["DP-1", "eDP-1"]);
    assert_eq!(
        state.settings.monitors.iter().find(|row| row.id == "DP-1"),
        Some(&disconnected_draft)
    );
    let internal = state
        .settings
        .monitors
        .iter()
        .find(|row| row.id == "eDP-1")
        .unwrap();
    assert_eq!((internal.x, internal.scale), (0, 2));
    assert_eq!(state.selected_monitor_id.as_deref(), Some("eDP-1"));
    save_display_layout(&mut state);
    let request = receiver.try_recv().unwrap();
    assert_eq!(
        request.monitors.iter().find(|row| row.id == "DP-1"),
        Some(&disconnected_draft)
    );
    receive_display_layout(&mut state, successful(request));
    assert!(!state.display_layout.is_pending());
}

#[test]
fn revision_zero_hand_edit_observed_after_submission_survives_commit_ten_completion() {
    let (_directory, mut state, mut receiver) = state();
    save_monitor_scale(&mut state, "primary", 2);
    let request = receiver.try_recv().unwrap();
    let old_commit = SettingsSnapshot::new(
        10,
        Settings {
            monitors: request.monitors.clone(),
            ..Settings::default()
        },
    )
    .unwrap();
    let mut hand_edit = Settings::default();
    hand_edit.monitors[0].scale = 3;
    hand_edit.monitors[1].x = 640;
    hand_edit.background_image_path = Some("hand edit.png".into());
    let latest = SettingsSnapshot::new(0, hand_edit).unwrap();
    crate::settings_refresh::apply_refresh(&mut state, (0, latest.clone()));
    assert_eq!(
        state.settings.monitors[0].scale, 2,
        "the pending preview remains local until completion"
    );
    assert_eq!(state.saved_monitors, latest.settings.monitors);
    receive_display_layout(
        &mut state,
        LayoutResponse {
            request,
            saved: Ok(old_commit),
        },
    );
    assert_eq!(
        state.settings.monitors[0].scale, 2,
        "ambiguous completion retains the acknowledged layout until a fresh read"
    );
    assert!(state.display_layout.is_pending());
    assert_eq!(state.saved_monitors, latest.settings.monitors);
    // An old queued watcher response cannot resolve commit ordering.
    crate::settings_refresh::apply_refresh(&mut state, (0, latest.clone()));
    assert_eq!(state.settings.monitors[0].scale, 2);
    // The forced GET is begun after acknowledgement, so its equality-only
    // revision-zero result authoritatively reconciles the committed layout.
    crate::settings_refresh::apply_refresh(&mut state, (1, latest.clone()));
    assert_eq!(state.settings.monitors, latest.settings.monitors);
    assert_eq!(state.saved_monitors, latest.settings.monitors);
    assert!(!state.display_layout.is_pending());
    assert_eq!(
        state.preferences.snapshot.as_ref().unwrap().cursor(),
        latest.cursor()
    );
    assert_eq!(
        state.settings.background_image_path.as_deref(),
        Some("hand edit.png")
    );
    assert!(
        receiver.try_recv().is_err(),
        "completion must not write an older layout back"
    );
    save_monitor_scale(&mut state, "secondary", 2);
    let next = receiver.try_recv().unwrap();
    assert_eq!(
        next.monitors[0].scale, 3,
        "the next edit starts from the observed hand edit"
    );
}
