use std::path::PathBuf;

use hearthspace_ipc::{
    BackgroundMode, MonitorConfig, client_set_background_image_path, client_set_background_mode,
};

use crate::{
    monitors::recalculate_monitor_positions_for_scale, settings_backend::save_display_layout,
    state::SettingsApp,
};

#[derive(Debug)]
pub(crate) enum MonitorEditorAction {
    Select {
        id: String,
        monitors: Vec<MonitorConfig>,
    },
    Drag {
        moved_id: String,
        monitors: Vec<MonitorConfig>,
        commit: bool,
    },
}

pub(crate) fn apply_monitor_editor_action(state: &mut SettingsApp, action: MonitorEditorAction) {
    match action {
        MonitorEditorAction::Select { id, monitors } => {
            state.settings.monitors = monitors;
            state.selected_monitor_id = Some(id);
            state.status.clear();
        }
        MonitorEditorAction::Drag {
            moved_id,
            monitors,
            commit,
        } => {
            state.display_layout.invalidate_preview();
            let moved_monitor_exists = monitors.iter().any(|monitor| monitor.id == moved_id);
            state.selected_monitor_id = Some(moved_id.clone());
            state.settings.monitors = monitors;

            if commit {
                save_display_layout(state);
            } else if moved_monitor_exists {
                state.status.clear();
            } else {
                state.status = format!("Unknown monitor: {moved_id}");
            }
        }
    }
}

pub(crate) fn keep_selected_monitor_available(state: &mut SettingsApp) {
    if state.selected_monitor_id.as_ref().is_some_and(|id| {
        state
            .settings
            .monitors
            .iter()
            .any(|monitor| &monitor.id == id)
    }) {
        return;
    }
    state.selected_monitor_id = state
        .settings
        .monitors
        .first()
        .map(|monitor| monitor.id.clone());
}

pub(crate) fn save_monitor_scale(state: &mut SettingsApp, id: &str, scale: i32) {
    let mut monitors = state.settings.monitors.clone();
    recalculate_monitor_positions_for_scale(&mut monitors, id, scale);
    state.settings.monitors = monitors;
    state.selected_monitor_id = Some(id.to_string());
    save_display_layout(state);
}

pub(crate) fn save_background_image_path(state: &mut SettingsApp, path: Option<PathBuf>) {
    let previous = state.settings.background_image_path.clone();
    let path = match path {
        Some(path) => match path.into_os_string().into_string() {
            Ok(path) => Some(path),
            Err(path) => {
                state.status = format!("Background path is not valid UTF-8: {path:?}");
                return;
            }
        },
        None => None,
    };
    state.settings.background_image_path = path.clone();

    match client_set_background_image_path(path.as_deref()) {
        Ok(settings) => {
            crate::settings_refresh::local_save_finished(state);
            state.settings.background_image_path = settings.background_image_path;
            state
                .background_preview
                .refresh(state.settings.background_image_path.as_deref());
            state.status.clear();
        }
        Err(error) => {
            state.settings.background_image_path = previous;
            state.status = format!("Failed to save background image: {error}");
        }
    }
}

pub(crate) fn save_background_mode(state: &mut SettingsApp, mode: BackgroundMode) {
    let previous = state.settings.background_mode;
    state.settings.background_mode = mode;

    match client_set_background_mode(mode) {
        Ok(settings) => {
            crate::settings_refresh::local_save_finished(state);
            state.settings.background_mode = settings.background_mode;
            state.status.clear();
        }
        Err(error) => {
            state.settings.background_mode = previous;
            state.status = format!("Failed to save background mode: {error}");
        }
    }
}
