use std::path::PathBuf;

use hearthspace_ipc::{MonitorConfig, client_set_background_image_path, client_set_monitor_scale};

use crate::{
    monitors::{recalculate_monitor_positions_for_scale, settings_with_live_outputs},
    settings_backend::{notify_compositor_settings_changed, save_monitor_positions},
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
            state.selected_monitor_id = Some(id.clone());
            state.status = format!("Selected monitor {id}");
        }
        MonitorEditorAction::Drag {
            moved_id,
            monitors,
            commit,
        } => {
            let moved_monitor = monitors
                .iter()
                .find(|monitor| monitor.id == moved_id)
                .cloned();
            state.selected_monitor_id = Some(moved_id.clone());
            state.settings.monitors = monitors;

            if commit {
                match save_monitor_positions(&state.settings.monitors) {
                    Ok(Some(settings)) => {
                        state.settings = settings_with_live_outputs(settings, &state.live_outputs);
                        keep_selected_monitor_available(state);
                        state.status = status_after_compositor_reload(format!(
                            "Saved monitor layout for {moved_id}"
                        ));
                    }
                    Ok(None) => {
                        state.status = status_after_compositor_reload(
                            "Saved empty monitor layout".to_string(),
                        );
                    }
                    Err(error) => state.status = format!("Failed to save monitor layout: {error}"),
                }
            } else if let Some(monitor) = moved_monitor {
                state.status = format!("Moving {} to {},{}", monitor.name, monitor.x, monitor.y);
            } else {
                state.status = format!("Unknown monitor: {moved_id}");
            }
        }
    }
}

fn keep_selected_monitor_available(state: &mut SettingsApp) {
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
    let previous = state.settings.clone();
    let mut monitors = state.settings.monitors.clone();
    recalculate_monitor_positions_for_scale(&mut monitors, id, scale);
    state.settings.monitors = monitors.clone();

    match client_set_monitor_scale(id, scale) {
        Ok(_) => match save_monitor_positions(&monitors) {
            Ok(Some(settings)) => {
                state.settings = settings_with_live_outputs(settings, &state.live_outputs);
                state.selected_monitor_id = Some(id.to_string());
                keep_selected_monitor_available(state);
                state.status = status_after_compositor_reload(format!(
                    "Saved monitor scale for {id}: {scale}x"
                ));
            }
            Ok(None) => {
                state.settings.monitors = monitors;
                state.selected_monitor_id = Some(id.to_string());
                state.status = status_after_compositor_reload(format!(
                    "Saved monitor scale for {id}: {scale}x"
                ));
            }
            Err(error) => {
                let _ = previous
                    .monitors
                    .iter()
                    .find(|monitor| monitor.id == id)
                    .map(|monitor| client_set_monitor_scale(id, monitor.scale));
                state.settings = previous;
                state.status = format!("Failed to save monitor positions after scale: {error}");
            }
        },
        Err(error) => {
            state.settings = previous;
            state.status = format!("Failed to save monitor scale: {error}");
        }
    }
}

fn status_after_compositor_reload(saved_message: String) -> String {
    match notify_compositor_settings_changed() {
        Ok(()) => saved_message,
        Err(error) => format!("{saved_message}; compositor reload failed: {error}"),
    }
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
            state.settings = settings_with_live_outputs(settings, &state.live_outputs);
            state.status = match notify_compositor_settings_changed() {
                Ok(()) => match &state.settings.background_image_path {
                    Some(path) => format!("Saved background image: {path}"),
                    None => "Restored default background image".to_string(),
                },
                Err(error) => format!("Saved background image; compositor reload failed: {error}"),
            };
        }
        Err(error) => {
            state.settings.background_image_path = previous;
            state.status = format!("Failed to save background image: {error}");
        }
    }
}
