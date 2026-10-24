use std::path::PathBuf;

use hearthspace_ipc::{MonitorConfig, Settings, client_get_settings};
use xilem::tokio::sync::mpsc::UnboundedSender;

use crate::monitors::LiveOutput;

pub(crate) struct SettingsApp {
    pub(crate) preferences: crate::settings_refresh::RefreshState,
    pub(crate) running: bool,
    pub(crate) navigation: crate::ui::navigation::State,
    pub(crate) appearance_theme: crate::appearance::Theme,
    pub(crate) activation_listener: Option<std::os::unix::net::UnixListener>,
    pub(crate) ai: crate::ai::AiSettings,
    pub(crate) favorites: crate::favorites::FavoriteSettings,
    pub(crate) frame_settings: crate::frame_settings::State,
    pub(crate) display_layout: crate::settings_backend::DisplayLayoutState,
    /// Non-display preferences plus the connected, editable display projection.
    pub(crate) settings: Settings,
    /// Complete authoritative display preferences, including disconnected outputs.
    pub(crate) saved_monitors: Vec<MonitorConfig>,
    pub(crate) output_observation: crate::wayland_outputs::Observation,
    pub(crate) live_outputs: Vec<LiveOutput>,
    pub(crate) selected_category: SettingsCategory,
    pub(crate) selected_monitor_id: Option<String>,
    pub(crate) background_preview: crate::background_preview::BackgroundPreview,
    pub(crate) background_picker_request_sender: Option<UnboundedSender<BackgroundPickerRequest>>,
    pub(crate) background_picker_pending: bool,
    pub(crate) status: String,
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
    let (settings, status) = match client_get_settings() {
        Ok(settings) => (settings, "Connected to hearthspace-settingsd".to_string()),
        Err(error) => (
            Settings::default(),
            format!("Settings backend unavailable: {error}"),
        ),
    };
    let saved_monitors = settings.monitors.clone();
    let selected_monitor_id = settings.monitors.first().map(|monitor| monitor.id.clone());

    SettingsApp {
        running: true,
        preferences: Default::default(),
        appearance_theme: Default::default(),
        navigation: Default::default(),
        activation_listener: None,
        ai: crate::ai::AiSettings::default(),
        favorites: crate::favorites::FavoriteSettings::load(),
        frame_settings: Default::default(),
        display_layout: Default::default(),
        settings,
        saved_monitors,
        output_observation: Default::default(),
        live_outputs: Vec::new(),
        selected_category,
        selected_monitor_id,
        background_preview: Default::default(),
        background_picker_request_sender: None,
        background_picker_pending: false,
        status,
    }
}

impl SettingsApp {
    pub(crate) fn palette(&self) -> hearthspace_ui_components::Palette {
        self.appearance_theme.palette(
            self.settings.appearance,
            self.settings.background_image_path.is_some(),
        )
    }

    pub(crate) fn select_category(&mut self, category: SettingsCategory) {
        self.navigation.open = false;
        self.favorites.cancel();
        if category == SettingsCategory::Gestures {
            self.favorites.reload();
        }
        if category == SettingsCategory::Ai {
            self.ai.request(crate::ai::Operation::Refresh);
        }
        if category == SettingsCategory::Background {
            self.background_preview
                .refresh(self.settings.background_image_path.as_deref());
        }
        if category == SettingsCategory::System {
            self.frame_settings
                .request(crate::frame_settings::Operation::Refresh);
        }
        self.selected_category = category;
    }
}

impl xilem::AppState for SettingsApp {
    fn keep_running(&self) -> bool {
        self.running
    }
}
