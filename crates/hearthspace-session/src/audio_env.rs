//! Resolve complete audio routing before direct launch and cold activation.
use std::{env, io};

const KEYS: &[&str] = &[
    "PULSE_SERVER",
    "PULSE_RUNTIME_PATH",
    "PIPEWIRE_RUNTIME_DIR",
    "PIPEWIRE_REMOTE",
    "PIPEWIRE_CORE",
];

pub(crate) fn audio_environment(host: &str) -> io::Result<Vec<(String, String)>> {
    let mut explicit = std::collections::BTreeMap::new();
    for key in KEYS {
        if let Some(value) = env::var_os(key) {
            explicit.insert(
                *key,
                value
                    .into_string()
                    .map_err(|_| io::Error::other("audio override is not UTF-8"))?,
            );
        }
    }
    Ok(resolved(host, |key| explicit.get(key).cloned()))
}

pub(crate) use hearthspace_desktop::environment::audio as resolved;
