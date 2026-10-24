//! One-way adapter for the old formatter's unquoted strings, followed by TOML parsing.
use std::io;

use hearthspace_ipc::{BackgroundMode, MonitorConfig, Settings, validate_monitor_configs};
use serde::Deserialize;

use super::invalid;

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct LegacyDocument {
    test: bool,
    background_mode: Option<BackgroundMode>,
    background_moves_with_canvas: Option<bool>,
    background_image_path: Option<String>,
    color_mode: Option<String>,
    accent_color: Option<String>,
    monitor: Option<Vec<LegacyMonitor>>,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct LegacyMonitor {
    id: String,
    name: String,
    width: i32,
    height: i32,
    scale: i32,
    x: i32,
    y: i32,
}

impl Default for LegacyMonitor {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            width: 1920,
            height: 1080,
            scale: 1,
            x: 0,
            y: 0,
        }
    }
}

pub(super) fn parse(contents: &str) -> io::Result<Settings> {
    let normalized = normalize(contents)?;
    let legacy: LegacyDocument = toml::from_str(&normalized).map_err(invalid)?;
    let mut settings = Settings {
        test: legacy.test,
        background_mode: legacy
            .background_mode
            .or_else(|| {
                legacy
                    .background_moves_with_canvas
                    .map(BackgroundMode::from_legacy_moves_with_canvas)
            })
            .unwrap_or_default(),
        background_image_path: legacy
            .background_image_path
            .filter(|path| path != "default"),
        ..Settings::default()
    };
    if let Some(mode) = legacy.color_mode {
        settings.appearance.color_mode = mode.parse().map_err(invalid)?;
    }
    if let Some(accent) = legacy.accent_color {
        settings.appearance.accent_color = accent.parse().map_err(invalid)?;
    }
    if let Some(monitors) = legacy.monitor {
        settings.monitors = monitors
            .into_iter()
            .map(|monitor| MonitorConfig {
                id: monitor.id,
                name: monitor.name,
                width: monitor.width,
                height: monitor.height,
                scale: monitor.scale,
                x: monitor.x,
                y: monitor.y,
            })
            .collect();
    }
    validate_monitor_configs(&settings.monitors).map_err(invalid)?;
    Ok(settings)
}

fn normalize(contents: &str) -> io::Result<String> {
    let mut normalized = String::new();
    let mut in_monitor = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.split('#').next().unwrap_or_default().trim() == "[[monitor]]" {
            in_monitor = true;
            normalized.push_str("[[monitor]]\n");
            continue;
        }
        let (key, value) = trimmed
            .split_once('=')
            .ok_or_else(|| invalid("invalid legacy settings line"))?;
        let key = key.trim();
        let value = value.trim();
        let string = match (in_monitor, key) {
            (
                false,
                "background_mode" | "background_image_path" | "color_mode" | "accent_color",
            )
            | (true, "id" | "name") => true,
            (false, "test" | "background_moves_with_canvas")
            | (true, "width" | "height" | "scale" | "x" | "y") => false,
            _ => {
                return Err(invalid(format!(
                    "unknown or versioned legacy settings field: {key}"
                )));
            }
        };
        normalized.push_str(key);
        normalized.push_str(" = ");
        if string && !value.starts_with(['\'', '"']) {
            // Enum comments were supported historically. Raw names/paths are data:
            // the old formatter did not escape '#' inside them.
            let value = if matches!(key, "background_mode" | "color_mode" | "accent_color") {
                value.split('#').next().unwrap_or_default().trim()
            } else {
                value
            };
            if value.is_empty() {
                return Err(invalid(format!("missing legacy setting value: {key}")));
            }
            normalized.push_str(&toml::Value::String(value.to_string()).to_string());
        } else {
            normalized.push_str(value);
        }
        normalized.push('\n');
    }
    Ok(normalized)
}
