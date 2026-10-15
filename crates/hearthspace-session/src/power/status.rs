use std::{fs, path::Path};

use serde::Serialize;

#[derive(Default, Serialize)]
pub(super) struct Status {
    pub phase: &'static str,
    pub connected: bool,
    pub sleep_inhibited: bool,
    pub lid_owned: bool,
    pub active: bool,
    pub sleep_count: u64,
    pub resume_count: u64,
    pub lock_confirmed: bool,
    pub session_id: String,
    pub last_error: Option<String>,
}

impl Status {
    pub fn save(&self, runtime: &Path) {
        let temporary = runtime.join("power.json.tmp");
        let result = serde_json::to_vec(self)
            .map_err(std::io::Error::other)
            .and_then(|bytes| fs::write(&temporary, bytes))
            .and_then(|()| fs::rename(temporary, runtime.join("power.json")));
        if let Err(error) = result {
            tracing::warn!(%error, "could not save power status");
        }
    }

    pub fn error(&mut self, error: impl std::fmt::Display) {
        let message = error.to_string();
        if self.last_error.as_ref() != Some(&message) {
            tracing::warn!(%message, "power coordination degraded");
        }
        self.last_error = Some(message);
        self.phase = "degraded";
    }
}

/// Treat any connected non-panel connector conservatively as a docked display.
/// Unlike public agent status, sysfs remains available while the session is locked.
pub(super) fn external_display() -> bool {
    fs::read_dir("/sys/class/drm").is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("card")
                && name.contains('-')
                && !["-eDP-", "-LVDS-", "-DSI-"]
                    .iter()
                    .any(|panel| name.contains(panel))
                && fs::read_to_string(entry.path().join("status"))
                    .is_ok_and(|s| s.trim() == "connected")
        })
    })
}
