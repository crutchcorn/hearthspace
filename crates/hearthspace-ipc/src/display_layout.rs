//! The display editor submits connected outputs together, including newly seen IDs.
use std::{collections::HashSet, io, path::Path, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{MonitorConfig, Settings};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayoutDocument {
    version: u32,
    monitors: Vec<MonitorConfig>,
}

pub fn client_set_display_layout(monitors: &[MonitorConfig]) -> io::Result<Settings> {
    client_set_display_layout_at(&crate::settings_socket_path()?, monitors)
}

pub fn client_set_display_layout_at(
    socket: &Path,
    monitors: &[MonitorConfig],
) -> io::Result<Settings> {
    let request = display_layout_request(monitors)?;
    let response = crate::transport::request_line(socket, &request, Duration::from_millis(1000))?;
    crate::parse_settings_response(&response)
}

pub fn display_layout_request(monitors: &[MonitorConfig]) -> io::Result<String> {
    validate_display_layout(monitors)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let document = LayoutDocument {
        version: 1,
        monitors: monitors.to_vec(),
    };
    let payload = serde_json::to_string(&document).map_err(io::Error::other)?;
    Ok(format!("set display-layout {payload}\n"))
}

pub(super) fn parse_request(payload: &str) -> Result<Vec<MonitorConfig>, String> {
    let document: LayoutDocument = serde_json::from_str(payload)
        .map_err(|error| format!("invalid display layout: {error}"))?;
    if document.version != 1 {
        return Err(format!(
            "unsupported display layout version: {}",
            document.version
        ));
    }
    validate_display_layout(&document.monitors)?;
    Ok(document.monitors)
}

/// Validate saved rows independently: disconnected monitors can share coordinates.
pub fn validate_monitor_configs(monitors: &[MonitorConfig]) -> Result<(), String> {
    let mut ids = HashSet::new();
    for monitor in monitors {
        if monitor.id.trim().is_empty() || monitor.id.contains('\0') {
            return Err("monitor id must be nonempty and cannot contain NUL".into());
        }
        if !ids.insert(&monitor.id) {
            return Err(format!("duplicate monitor id: {}", monitor.id));
        }
        if monitor.width <= 0 || monitor.height <= 0 || monitor.scale <= 0 {
            return Err(format!(
                "monitor {} dimensions and scale must be positive",
                monitor.id
            ));
        }
        let (right, bottom) = edges(monitor);
        if right > i64::from(i32::MAX) || bottom > i64::from(i32::MAX) {
            return Err(format!(
                "monitor {} extends beyond supported coordinates",
                monitor.id
            ));
        }
    }
    Ok(())
}

/// All connected rectangles must be valid together before any preference is changed.
pub fn validate_display_layout(monitors: &[MonitorConfig]) -> Result<(), String> {
    validate_monitor_configs(monitors)?;
    for (index, monitor) in monitors.iter().enumerate() {
        let (right, bottom) = edges(monitor);
        for other in &monitors[index + 1..] {
            let (other_right, other_bottom) = edges(other);
            if i64::from(monitor.x) < other_right
                && right > i64::from(other.x)
                && i64::from(monitor.y) < other_bottom
                && bottom > i64::from(other.y)
            {
                return Err(format!("monitors {} and {} overlap", monitor.id, other.id));
            }
        }
    }
    Ok(())
}

fn edges(monitor: &MonitorConfig) -> (i64, i64) {
    let scale = i64::from(monitor.scale);
    (
        i64::from(monitor.x) + (i64::from(monitor.width) + scale - 1) / scale,
        i64::from(monitor.y) + (i64::from(monitor.height) + scale - 1) / scale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_layout_round_trips_names_ids_and_newline_strings() {
        let mut monitors = Settings::default().monitors;
        monitors[0].id = "DP #1 – desk".into();
        monitors[0].name = "Display \"虹\"\n#1".into();
        let request = display_layout_request(&monitors).unwrap();
        assert_eq!(request.lines().count(), 1);
        assert_eq!(
            crate::parse_settings_request(&request).unwrap(),
            crate::SettingsRequest::SetDisplayLayout(monitors)
        );
    }

    #[test]
    fn layout_rejects_duplicates_overlaps_and_coordinate_overflow() {
        let mut monitors = Settings::default().monitors;
        monitors[1].id = monitors[0].id.clone();
        assert!(
            validate_display_layout(&monitors)
                .unwrap_err()
                .contains("duplicate")
        );
        monitors[1].id = "second".into();
        monitors[1].x = 0;
        assert!(
            validate_display_layout(&monitors)
                .unwrap_err()
                .contains("overlap")
        );
        monitors[1].x = i32::MAX;
        assert!(
            validate_display_layout(&monitors)
                .unwrap_err()
                .contains("coordinates")
        );
    }

    #[test]
    fn layout_rejects_unknown_versions_and_incomplete_rows() {
        assert!(parse_request(r#"{"version":2,"monitors":[]}"#).is_err());
        assert!(parse_request(r#"{"version":1,"monitors":[{"id":"third"}]}"#).is_err());
        assert!(parse_request(r#"{"version":1,"monitors":[],"ignored":true}"#).is_err());
    }

    #[test]
    fn settings_response_preserves_literal_default_and_empty_strings() {
        for path in ["default", "", "/tmp/\"虹\" #\n.png"] {
            let mut settings = Settings {
                background_image_path: Some(path.into()),
                ..Settings::default()
            };
            settings.monitors[0].name.clear();
            let response = crate::settings_response(settings.clone());
            assert_eq!(crate::parse_settings_response(&response).unwrap(), settings);
            let token = crate::encode_background_path(path);
            assert_eq!(
                crate::parse_settings_request(&format!("set background-image-path {token}\n"))
                    .unwrap(),
                crate::SettingsRequest::SetBackgroundImagePath(Some(path.into()))
            );
        }
    }
}
