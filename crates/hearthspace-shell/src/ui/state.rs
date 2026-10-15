use std::path::PathBuf;

use hearthspace_common::{
    CANVAS_MAP_APP_ID, LAUNCHER_APP_ID, SHELL_BAR_APP_ID, canvas_map_app_id_for_output,
    launcher_app_id_for_output, shell_bar_app_id_for_output,
};
use xilem::{AppState, WindowId};

use crate::app_catalog::AppCatalog;

use super::{
    constants::FALLBACK_OUTPUT_KEY, minimap::CanvasSnapshot, notifications::Notification,
    window_catalog::RunningWindow,
};

/// Reactive state backing the shell view.
pub(super) struct ShellState {
    pub(super) command_socket: PathBuf,
    pub(super) catalog: AppCatalog,
    pub(super) windows: Vec<RunningWindow>,
    pub(super) windows_error: Option<String>,
    pub(super) canvas: Option<CanvasSnapshot>,
    pub(super) canvas_error: Option<String>,
    pub(super) canvas_dragging: bool,
    pub(super) canvas_revision: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub(super) clock: String,
    pub(super) notifications_socket: Option<PathBuf>,
    pub(super) notifications: Vec<Notification>,
    pub(super) notifications_error: Option<String>,
    pub(super) outputs: Vec<ShellOutputInstance>,
    pub(super) watcher_window_id: WindowId,
    running: bool,
}

#[derive(Debug)]
pub(super) struct ShellOutputInstance {
    pub(super) output_name: Option<String>,
    pub(super) query: String,
    pub(super) show_windows: bool,
    pub(super) confirm_logout: bool,
    pub(super) show_notifications: bool,
    pub(super) show_map: bool,
    pub(super) launch_error: Option<String>,
    pub(super) bar_window_id: WindowId,
    pub(super) launcher_window_id: WindowId,
    pub(super) map_window_id: WindowId,
}

#[derive(Debug)]
pub(super) struct OutputNamesChanged {
    pub(super) names: Vec<String>,
}

impl AppState for ShellState {
    fn keep_running(&self) -> bool {
        self.running
    }
}

impl ShellState {
    pub(super) fn new(command_socket: PathBuf, output_names: Vec<String>) -> Self {
        let mut state = Self {
            command_socket,
            catalog: AppCatalog::load(),
            windows: Vec::new(),
            windows_error: None,
            canvas: None,
            canvas_error: None,
            canvas_dragging: false,
            canvas_revision: Default::default(),
            clock: super::session_controls::current_clock(),
            notifications_socket: std::env::var_os("XDG_RUNTIME_DIR")
                .map(|dir| PathBuf::from(dir).join("hearthspace-notifications.sock")),
            notifications: Vec::new(),
            notifications_error: None,
            outputs: Vec::new(),
            watcher_window_id: WindowId::next(),
            running: true,
        };
        state.apply_output_names(output_names);
        state
    }

    pub(super) fn stop(&mut self) {
        self.running = false;
    }

    pub(super) fn apply_output_names(&mut self, output_names: Vec<String>) {
        let targets = normalized_output_targets(output_names);
        let mut existing = std::mem::take(&mut self.outputs);
        let mut outputs = Vec::with_capacity(targets.len());

        for output_name in targets {
            if let Some(index) = existing
                .iter()
                .position(|instance| instance.output_name == output_name)
            {
                outputs.push(existing.remove(index));
            } else {
                outputs.push(ShellOutputInstance::new(output_name));
            }
        }

        self.outputs = outputs;
        if !self.outputs.iter().any(|output| output.show_map) {
            self.canvas_dragging = false;
        }
    }

    pub(super) fn finish_launch(&mut self, output_key: &str, result: Result<(), String>) {
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key)
        {
            match result {
                Ok(()) => {
                    self.canvas_dragging = false;
                    output.query.clear();
                    output.show_windows = false;
                    output.show_notifications = false;
                    output.show_map = false;
                    output.launch_error = None;
                }
                Err(error) => output.launch_error = Some(error),
            }
        }
    }

    pub(super) fn toggle_windows(&mut self, output_key: &str) {
        self.canvas_dragging = false;
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key)
        {
            output.show_windows = !output.show_windows;
            output.show_map = false;
            output.show_notifications = false;
            output.query.clear();
            output.launch_error = None;
        }
    }

    pub(super) fn dismiss_launcher(&mut self, output_key: &str) {
        self.canvas_dragging = false;
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key)
        {
            output.show_windows = false;
            output.show_map = false;
            output.show_notifications = false;
            output.confirm_logout = false;
            output.query.clear();
            output.launch_error = None;
        }
    }

    pub(super) fn change_query(&mut self, output_key: &str, query: String) {
        self.canvas_dragging = false;
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key)
        {
            output.query = query;
            output.show_map = false;
            output.show_notifications = false;
            output.launch_error = None;
        }
    }

    pub(super) fn toggle_map(&mut self, output_key: &str) {
        self.canvas_dragging = false;
        let open = self
            .outputs
            .iter()
            .any(|output| output.key() == output_key && !output.show_map);
        for output in &mut self.outputs {
            output.show_map = open && output.key() == output_key;
            if output.show_map {
                output.query.clear();
                output.show_windows = false;
                output.show_notifications = false;
                output.confirm_logout = false;
                output.launch_error = None;
            }
        }
        self.canvas_error = None;
    }

    pub(super) fn dismiss_map(&mut self, output_key: &str) {
        self.canvas_dragging = false;
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key)
        {
            output.show_map = false;
        }
    }
}

impl ShellOutputInstance {
    fn new(output_name: Option<String>) -> Self {
        Self {
            output_name,
            query: String::new(),
            show_windows: false,
            confirm_logout: false,
            show_notifications: false,
            show_map: false,
            launch_error: None,
            bar_window_id: WindowId::next(),
            launcher_window_id: WindowId::next(),
            map_window_id: WindowId::next(),
        }
    }

    pub(super) fn key(&self) -> &str {
        self.output_name.as_deref().unwrap_or(FALLBACK_OUTPUT_KEY)
    }

    pub(super) fn bar_app_id(&self) -> String {
        self.output_name
            .as_deref()
            .map(shell_bar_app_id_for_output)
            .unwrap_or_else(|| SHELL_BAR_APP_ID.to_string())
    }

    pub(super) fn launcher_app_id(&self) -> String {
        self.output_name
            .as_deref()
            .map(launcher_app_id_for_output)
            .unwrap_or_else(|| LAUNCHER_APP_ID.to_string())
    }

    pub(super) fn map_app_id(&self) -> String {
        self.output_name
            .as_deref()
            .map(canvas_map_app_id_for_output)
            .unwrap_or_else(|| CANVAS_MAP_APP_ID.to_string())
    }
}

fn normalized_output_targets(output_names: Vec<String>) -> Vec<Option<String>> {
    let mut targets: Vec<Option<String>> = Vec::new();
    for name in output_names {
        if name.is_empty()
            || targets
                .iter()
                .any(|target| target.as_deref() == Some(&name))
        {
            continue;
        }
        targets.push(Some(name));
    }

    if targets.is_empty() {
        targets.push(None);
    }
    targets
}

#[cfg(test)]
mod tests {
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
        let bar_window_id = state.outputs[0].bar_window_id;

        state.apply_output_names(vec!["eDP-1".to_string(), "HDMI-A-1".to_string()]);

        assert_eq!(state.outputs[0].bar_window_id, bar_window_id);
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
        state.outputs[0].show_windows = true;
        state.outputs[0].query = "notes".into();
        state.toggle_map("eDP-1");
        assert!(state.outputs[0].show_map);
        assert!(!state.outputs[0].show_notifications);
        assert!(!state.outputs[0].show_windows);
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
        state.toggle_windows("eDP-1");
        assert!(!state.outputs[0].show_map);
        state.toggle_map("eDP-1");
        state.dismiss_launcher("eDP-1");
        assert!(!state.outputs[0].show_map);
    }
}
