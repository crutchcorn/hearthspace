//! Committed preferences, independent of each consumer's live projections.
//! Daemon commits advance revision; the fingerprint also detects valid hand edits
//! without rewriting legacy files on read. Cursors are equality tokens, not clocks.
use crate::Settings;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io, path::Path, time::Duration};

pub const MAX_WAIT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsCursor {
    pub revision: u64,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsSnapshot {
    pub revision: u64,
    pub fingerprint: String,
    pub settings: Settings,
}

impl SettingsSnapshot {
    pub fn new(revision: u64, settings: Settings) -> io::Result<Self> {
        crate::validate_monitor_configs(&settings.monitors).map_err(io::Error::other)?;
        let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&settings)?));
        Ok(Self {
            revision,
            fingerprint,
            settings,
        })
    }

    pub fn cursor(&self) -> SettingsCursor {
        SettingsCursor {
            revision: self.revision,
            fingerprint: self.fingerprint.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchRequest {
    pub version: u32,
    pub after: SettingsCursor,
    pub wait_ms: u64,
}

impl WatchRequest {
    pub fn parse(payload: &str) -> Result<Self, String> {
        let request: Self = serde_json::from_str(payload).map_err(|error| error.to_string())?;
        if request.version != 1
            || request.wait_ms > MAX_WAIT.as_millis() as u64
            || request.after.fingerprint.len() != 64
            || !request
                .after
                .fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("invalid settings watch version, cursor, or wait limit".into());
        }
        Ok(request)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    version: u32,
    snapshot: Option<SettingsSnapshot>,
}

pub fn snapshot_response(snapshot: Option<SettingsSnapshot>) -> io::Result<String> {
    Ok(format!(
        "ok snapshot {}\n",
        serde_json::to_string(&Response {
            version: 1,
            snapshot
        })?
    ))
}

pub fn client_get_snapshot() -> io::Result<SettingsSnapshot> {
    client_get_snapshot_at(&crate::settings_socket_path()?)
}

pub fn client_get_snapshot_at(socket: &Path) -> io::Result<SettingsSnapshot> {
    let line =
        crate::transport::request_line(socket, "get snapshot\n", Duration::from_millis(500))?;
    parse_response(&line)?.ok_or_else(|| io::Error::other("snapshot missing from get response"))
}

/// Same atomic display mutation as the legacy setter, with its committed cursor.
pub fn client_set_display_layout_snapshot_at(
    socket: &Path,
    monitors: &[crate::MonitorConfig],
) -> io::Result<SettingsSnapshot> {
    let request = format!("commit {}", crate::display_layout_request(monitors)?);
    let line = crate::transport::request_line(socket, &request, Duration::from_millis(1000))?;
    parse_response(&line)?.ok_or_else(|| io::Error::other("snapshot missing from commit response"))
}

pub fn client_wait_for_change(
    after: &SettingsCursor,
    wait: Duration,
) -> io::Result<Option<SettingsSnapshot>> {
    client_wait_for_change_at(&crate::settings_socket_path()?, after, wait)
}

/// One absolute transport deadline, including connect and the requested server wait.
/// Call on a worker. Short waits permit bounded cancellation; None means unchanged.
pub fn client_wait_for_change_at(
    socket: &Path,
    after: &SettingsCursor,
    wait: Duration,
) -> io::Result<Option<SettingsSnapshot>> {
    if wait > MAX_WAIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "settings wait exceeds 30 seconds",
        ));
    }
    let request = WatchRequest {
        version: 1,
        after: after.clone(),
        wait_ms: wait.as_millis() as u64,
    };
    let payload = serde_json::to_string(&request)?;
    WatchRequest::parse(&payload).map_err(io::Error::other)?;
    let line = crate::transport::request_line(
        socket,
        &format!("watch settings {payload}\n"),
        wait + Duration::from_millis(500),
    )?;
    parse_response(&line)
}

fn parse_response(line: &str) -> io::Result<Option<SettingsSnapshot>> {
    let payload = line
        .trim()
        .strip_prefix("ok snapshot ")
        .ok_or_else(|| io::Error::other(line.trim().to_string()))?;
    let response: Response = serde_json::from_str(payload)?;
    if response.version != 1 {
        return Err(io::Error::other("unsupported settings snapshot version"));
    }
    if let Some(snapshot) = &response.snapshot {
        let expected = SettingsSnapshot::new(snapshot.revision, snapshot.settings.clone())?;
        if expected.fingerprint != snapshot.fingerprint {
            return Err(io::Error::other("settings fingerprint mismatch"));
        }
    }
    Ok(response.snapshot)
}

// Common appearance intentionally has no serialization dependency. The settings
// protocol owns its string representation, including strict enum validation.
pub(crate) mod appearance {
    use super::*;
    use crate::Appearance;
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wire {
        color_mode: String,
        accent_color: String,
    }
    pub fn serialize<S: serde::Serializer>(
        value: &Appearance,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Wire {
            color_mode: value.color_mode.to_string(),
            accent_color: value.accent_color.to_string(),
        }
        .serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Appearance, D::Error> {
        let value = Wire::deserialize(deserializer)?;
        Ok(Appearance {
            color_mode: value.color_mode.parse().map_err(serde::de::Error::custom)?,
            accent_color: value
                .accent_color
                .parse()
                .map_err(serde::de::Error::custom)?,
        })
    }
}

#[cfg(test)]
mod tests;
