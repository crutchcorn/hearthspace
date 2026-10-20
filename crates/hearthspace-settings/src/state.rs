use std::path::PathBuf;

use hearthspace_ipc::{Settings, client_get_settings};
use xilem::tokio::sync::mpsc::UnboundedSender;

use crate::{
    monitors::{LiveOutput, settings_with_live_outputs},
    wayland_outputs::live_wayland_outputs,
};

pub(crate) struct SettingsApp {
    pub(crate) activation_listener: Option<std::os::unix::net::UnixListener>,
    pub(crate) ai: crate::ai::AiSettings,
    pub(crate) favorites: crate::favorites::FavoriteSettings,
    pub(crate) settings: Settings,
    pub(crate) live_outputs: Vec<LiveOutput>,
    pub(crate) selected_category: SettingsCategory,
    pub(crate) selected_monitor_id: Option<String>,
    pub(crate) background_picker_request_sender: Option<UnboundedSender<BackgroundPickerRequest>>,
    pub(crate) background_picker_pending: bool,
    pub(crate) background_fit_open: bool,
    pub(crate) status: String,
    pub(crate) config_label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SettingsCategory {
    Appearance,
    Displays,
    Background,
    Gestures,
    Ai,
    System,
}

impl SettingsCategory {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Displays => "Displays",
            Self::Appearance => "Appearance",
            Self::Background => "Background",
            Self::Gestures => "Gestures",
            Self::Ai => "AI",
            Self::System => "System",
        }
    }

    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::Displays => "Arrange your screens and adjust each display.",
            Self::Appearance => "Light and color, shaped around your space.",
            Self::Background => "A backdrop for your workspace.",
            Self::Gestures => "Draw shortcuts for your apps.",
            Self::Ai => "Find related windows across your canvas.",
            Self::System => "Your system tray and canvas controls.",
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
        activation_listener: None,
        ai: crate::ai::AiSettings::default(),
        favorites: crate::favorites::FavoriteSettings::load(),
        settings,
        live_outputs,
        selected_category,
        selected_monitor_id,
        background_picker_request_sender: None,
        background_picker_pending: false,
        background_fit_open: false,
        status,
        config_label,
    }
}

impl SettingsApp {
    pub(crate) fn select_category(&mut self, category: SettingsCategory) {
        self.favorites.cancel();
        if category == SettingsCategory::Gestures {
            self.favorites.reload();
        }
        if category == SettingsCategory::Ai {
            self.ai.request(crate::ai::Operation::Refresh);
        }
        self.selected_category = category;
    }
}
