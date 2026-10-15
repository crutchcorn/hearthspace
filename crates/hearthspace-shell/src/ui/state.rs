use std::path::PathBuf;

use hearthspace_common::{
    CANVAS_MAP_APP_ID, LAUNCHER_APP_ID, NOTIFICATIONS_APP_ID, POWER_MENU_APP_ID, SHELL_BAR_APP_ID,
    canvas_map_app_id_for_output, launcher_app_id_for_output, launcher_app_id_for_request,
    notifications_app_id_for_output, power_menu_app_id_for_output, shell_bar_app_id_for_output,
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
    pub(super) shell_events: Option<std::sync::Arc<super::shell_events::ShellEvents>>,
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
    pub(super) show_search: bool,
    pub(super) search_request_token: Option<u64>,
    pub(super) show_power: bool,
    pub(super) confirm_logout: bool,
    pub(super) show_notifications: bool,
    pub(super) show_map: bool,
    pub(super) launch_error: Option<String>,
    pub(super) bar_window_id: WindowId,
    pub(super) launcher_window_id: WindowId,
    pub(super) map_window_id: WindowId,
    pub(super) notifications_window_id: WindowId,
    pub(super) power_window_id: WindowId,
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
            shell_events: None,
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
        match result {
            Ok(()) => self.dismiss_launcher(output_key),
            Err(error) => {
                if let Some(output) = self
                    .outputs
                    .iter_mut()
                    .find(|output| output.key() == output_key)
                {
                    output.launch_error = Some(error);
                }
            }
        }
    }

    /// Only one palette can own keyboard focus, including across outputs.
    fn toggle_panel(&mut self, output_key: &str, panel: Panel) {
        let open = self
            .outputs
            .iter()
            .any(|output| output.key() == output_key && !output.panel_is_open(panel));
        self.dismiss_panels();
        if open
            && let Some(output) = self
                .outputs
                .iter_mut()
                .find(|output| output.key() == output_key)
        {
            match panel {
                Panel::Search => output.show_search = true,
                Panel::Notifications => output.show_notifications = true,
                Panel::Map => output.show_map = true,
                Panel::Power => output.show_power = true,
            }
        }
    }

    pub(super) fn dismiss_panels(&mut self) {
        self.canvas_dragging = false;
        for output in &mut self.outputs {
            output.dismiss_panels();
        }
    }

    pub(super) fn toggle_search(&mut self, output_key: &str) {
        self.toggle_panel(output_key, Panel::Search);
    }

    pub(super) fn toggle_notifications(&mut self, output_key: &str) {
        self.toggle_panel(output_key, Panel::Notifications);
    }

    pub(super) fn toggle_power(&mut self, output_key: &str) {
        self.toggle_panel(output_key, Panel::Power);
    }

    pub(super) fn dismiss_launcher(&mut self, output_key: &str) {
        self.canvas_dragging = false;
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key)
        {
            output.dismiss_panels();
        }
    }

    pub(super) fn dismiss_search(&mut self, output_key: &str) {
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key && output.show_search)
        {
            output.show_search = false;
            output.search_request_token = None;
            output.query.clear();
            output.launch_error = None;
        }
    }

    pub(super) fn dismiss_notifications(&mut self, output_key: &str) {
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key)
        {
            output.show_notifications = false;
        }
    }

    pub(super) fn dismiss_power(&mut self, output_key: &str) {
        if let Some(output) = self
            .outputs
            .iter_mut()
            .find(|output| output.key() == output_key && output.show_power)
        {
            output.show_power = false;
            output.confirm_logout = false;
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
            output.show_search = true;
            output.show_map = false;
            output.show_power = false;
            output.show_notifications = false;
            output.confirm_logout = false;
            output.launch_error = None;
        }
    }

    pub(super) fn toggle_map(&mut self, output_key: &str) {
        self.toggle_panel(output_key, Panel::Map);
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

#[derive(Clone, Copy)]
enum Panel {
    Search,
    Notifications,
    Map,
    Power,
}

impl ShellOutputInstance {
    fn new(output_name: Option<String>) -> Self {
        Self {
            output_name,
            query: String::new(),
            show_search: false,
            search_request_token: None,
            show_power: false,
            confirm_logout: false,
            show_notifications: false,
            show_map: false,
            launch_error: None,
            bar_window_id: WindowId::next(),
            launcher_window_id: WindowId::next(),
            map_window_id: WindowId::next(),
            notifications_window_id: WindowId::next(),
            power_window_id: WindowId::next(),
        }
    }

    fn panel_is_open(&self, panel: Panel) -> bool {
        match panel {
            Panel::Search => self.show_search,
            Panel::Notifications => self.show_notifications,
            Panel::Map => self.show_map,
            Panel::Power => self.show_power,
        }
    }

    fn dismiss_panels(&mut self) {
        self.show_search = false;
        self.search_request_token = None;
        self.show_notifications = false;
        self.show_map = false;
        self.show_power = false;
        self.confirm_logout = false;
        self.query.clear();
        self.launch_error = None;
    }

    pub(super) fn notifications_app_id(&self) -> String {
        self.output_name
            .as_deref()
            .map(notifications_app_id_for_output)
            .unwrap_or_else(|| NOTIFICATIONS_APP_ID.to_string())
    }

    pub(super) fn power_app_id(&self) -> String {
        self.output_name
            .as_deref()
            .map(power_menu_app_id_for_output)
            .unwrap_or_else(|| POWER_MENU_APP_ID.to_string())
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
        if let Some(token) = self.search_request_token {
            return launcher_app_id_for_request(self.output_name.as_deref(), token);
        }
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
#[path = "state_tests.rs"]
mod tests;
