//! Versioned TOML preferences. Reads never migrate; the next successful save does.
use std::io;

use hearthspace_ipc::{
    Appearance, BackgroundMode, MonitorConfig, Settings, validate_monitor_configs,
};
use serde::{Deserialize, Serialize};

use super::invalid;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    test: bool,
    #[serde(default)]
    background_mode: BackgroundMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    background_image_path: Option<String>,
    #[serde(default = "default_color")]
    color_mode: String,
    #[serde(default = "default_accent")]
    accent_color: String,
    #[serde(default = "default_monitors", rename = "monitor")]
    monitors: Vec<MonitorConfig>,
}

#[cfg(test)]
pub(super) fn parse(contents: &str) -> io::Result<Settings> {
    parse_snapshot(contents).map(|snapshot| snapshot.settings)
}

pub(super) fn parse_snapshot(
    contents: &str,
) -> io::Result<hearthspace_ipc::settings_snapshot::SettingsSnapshot> {
    match toml::from_str::<toml::Table>(contents) {
        Ok(table) if table.contains_key("version") => {
            let document: Document = toml::from_str(contents).map_err(invalid)?;
            if document.version != 1 {
                return Err(invalid(format!(
                    "unsupported settings version: {}",
                    document.version
                )));
            }
            let settings = Settings {
                test: document.test,
                appearance: Appearance {
                    color_mode: document.color_mode.parse().map_err(invalid)?,
                    accent_color: document.accent_color.parse().map_err(invalid)?,
                },
                background_mode: document.background_mode,
                background_image_path: document.background_image_path,
                monitors: document.monitors,
            };
            validate_monitor_configs(&settings.monitors).map_err(invalid)?;
            hearthspace_ipc::settings_snapshot::SettingsSnapshot::new(document.revision, settings)
        }
        // The legacy reader rejects any version field, including malformed ones.
        _ => hearthspace_ipc::settings_snapshot::SettingsSnapshot::new(
            0,
            super::legacy::parse(contents)?,
        ),
    }
}

pub(super) fn format(
    snapshot: hearthspace_ipc::settings_snapshot::SettingsSnapshot,
) -> io::Result<String> {
    let settings = snapshot.settings;
    validate_monitor_configs(&settings.monitors).map_err(invalid)?;
    toml::to_string_pretty(&Document {
        version: 1,
        revision: snapshot.revision,
        test: settings.test,
        background_mode: settings.background_mode,
        background_image_path: settings.background_image_path,
        color_mode: settings.appearance.color_mode.to_string(),
        accent_color: settings.appearance.accent_color.to_string(),
        monitors: settings.monitors,
    })
    .map_err(invalid)
}

fn default_color() -> String {
    Settings::default().appearance.color_mode.to_string()
}

fn default_accent() -> String {
    Settings::default().appearance.accent_color.to_string()
}

fn default_monitors() -> Vec<MonitorConfig> {
    Settings::default().monitors
}
