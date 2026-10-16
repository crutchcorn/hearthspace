use std::path::PathBuf;

use hearthspace_ipc::{Settings, client_get_settings};
use xilem::tokio::sync::mpsc::UnboundedSender;

use crate::{
    monitors::{LiveOutput, settings_with_live_outputs},
    wayland_outputs::live_wayland_outputs,
};

pub(crate) struct SettingsApp {
    pub(crate) favorites: crate::favorites::FavoriteSettings,
    pub(crate) settings: Settings,
    pub(crate) live_outputs: Vec<LiveOutput>,
    pub(crate) selected_category: SettingsCategory,
    pub(crate) selected_monitor_id: Option<String>,
    pub(crate) background_picker_request_sender: Option<UnboundedSender<BackgroundPickerRequest>>,
    pub(crate) background_picker_pending: bool,
    pub(crate) status: String,
    pub(crate) config_label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SettingsCategory {
    Display,
    Appearance,
    Search,
    Advanced,
}

impl SettingsCategory {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Display => "Display",
            Self::Appearance => "Appearance",
            Self::Search => "Search",
            Self::Advanced => "Advanced",
        }
    }

    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::Display => "Monitor placement and scale",
            Self::Appearance => "Canvas background image and fit",
            Self::Search => "Search and app launch gestures",
            Self::Advanced => "Diagnostics and configuration",
        }
    }
}

#[derive(Debug)]
pub(crate) enum BackgroundPickerRequest {
    PickFile,
}

#[derive(Debug)]
pub(crate) enum BackgroundPickerResponse {
    Selected(PathBuf),
    Cancelled,
    Failed(String),
}

pub(crate) fn initial_state(selected_category: SettingsCategory) -> SettingsApp {
    let (mut settings, mut status) = match client_get_settings() {
        Ok(settings) => (settings, "Connected to hearthspace-settingsd".to_string()),
        Err(error) => (
            Settings::default(),
            format!("Settings backend unavailable: {error}"),
        ),
    };
    let live_outputs = match live_wayland_outputs() {
        Ok(outputs) => outputs,
        Err(error) => {
            status.push_str(&format!("; live monitors unavailable: {error}"));
            Vec::new()
        }
    };
    settings = settings_with_live_outputs(settings, &live_outputs);
    let selected_monitor_id = settings.monitors.first().map(|monitor| monitor.id.clone());

    let config_label = match hearthspace_common::config_path() {
        Ok(path) => format!("Config: {}", path.display()),
        Err(error) => format!("Config path unavailable: {error}"),
    };

    SettingsApp {
        favorites: crate::favorites::FavoriteSettings::load(),
        settings,
        live_outputs,
        selected_category,
        selected_monitor_id,
        background_picker_request_sender: None,
        background_picker_pending: false,
        status,
        config_label,
    }
}

impl SettingsApp {
    pub(crate) fn select_category(&mut self, category: SettingsCategory) {
        self.favorites.cancel();
        if category == SettingsCategory::Search {
            self.favorites.reload();
        }
        self.selected_category = category;
    }
}
