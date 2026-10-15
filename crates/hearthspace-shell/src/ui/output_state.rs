use super::{
    constants::FALLBACK_OUTPUT_KEY,
    state::{Panel, ShellOutputInstance},
};
use hearthspace_common::{
    CANVAS_MAP_APP_ID, LAUNCHER_APP_ID, NOTIFICATIONS_APP_ID, POWER_MENU_APP_ID, SHELL_BAR_APP_ID,
    canvas_map_app_id_for_output, launcher_app_id_for_output, launcher_app_id_for_request,
    notifications_app_id_for_output, power_menu_app_id_for_output, shell_bar_app_id_for_output,
};
use xilem::WindowId;

impl ShellOutputInstance {
    pub(super) fn new(output_name: Option<String>) -> Self {
        Self {
            popup_measurements: Default::default(),
            output_name,
            query: String::new(),
            selected_result: None,
            selected_result_index: 0,
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
            tray: Default::default(),
        }
    }

    pub(super) fn panel_is_open(&self, panel: Panel) -> bool {
        match panel {
            Panel::Search => self.show_search,
            Panel::Notifications => self.show_notifications,
            Panel::Map => self.show_map,
            Panel::Power => self.show_power,
        }
    }

    pub(super) fn dismiss_panels(&mut self) {
        self.tray.dismiss();
        self.show_search = false;
        self.search_request_token = None;
        self.show_notifications = false;
        self.show_map = false;
        self.show_power = false;
        self.confirm_logout = false;
        self.query.clear();
        self.selected_result = None;
        self.selected_result_index = 0;
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
