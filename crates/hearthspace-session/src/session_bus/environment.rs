use std::{collections::BTreeMap, io, path::Path};

use crate::session_env::{SessionEnv, SessionPaths};

use super::cleanup::{KEYS, MARKER};

pub(super) fn values(
    paths: &SessionPaths,
    session: &SessionEnv,
    private: bool,
    profile: &[(String, String)],
) -> io::Result<BTreeMap<String, String>> {
    let mut values: BTreeMap<_, _> = session
        .vars
        .iter()
        .filter(|(key, _)| private || KEYS.contains(&key.as_str()))
        .cloned()
        .collect();
    let bus = values
        .get("HEARTHSPACE_SESSION_BUS_ADDRESS")
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| io::Error::other("selected session bus is missing"))?;
    values.insert("DBUS_SESSION_BUS_ADDRESS".into(), bus);
    if private {
        values.extend(profile.iter().cloned());
    } else {
        let (runtime, display) = hearthspace_desktop::environment::connection(
            &paths.host_runtime_dir,
            &paths.session_dir,
            &session.wayland_display,
            false,
        )?;
        values.insert("XDG_RUNTIME_DIR".into(), runtime);
        values.insert("WAYLAND_DISPLAY".into(), display);
        values
            .entry(MARKER.into())
            .or_insert(path(&paths.session_dir)?);
        // Absence in this launch must not retain another session's integration.
        for key in KEYS {
            if !matches!(
                *key,
                "WAYLAND_SOCKET"
                    | "AT_SPI_BUS_ADDRESS"
                    | "PULSE_SERVER"
                    | "PULSE_RUNTIME_PATH"
                    | "PIPEWIRE_RUNTIME_DIR"
                    | "PIPEWIRE_REMOTE"
                    | "PIPEWIRE_CORE"
            ) {
                values.entry((*key).into()).or_default();
            }
        }
    }
    // Resolve the same complete audio routing as direct children. D-Bus only
    // updates keys, so omitting a complement can preserve a stale host endpoint.
    let audio = crate::audio_env::resolved(&path(&paths.host_runtime_dir)?, |key| {
        values.get(key).cloned()
    });
    values.extend(audio);
    values.insert("DISPLAY".into(), String::new());
    // Empty is not unset: libwayland interprets WAYLAND_SOCKET as an FD; GTK4
    // treats even an empty AT_SPI_BUS_ADDRESS as overriding bus discovery.
    // The manager is explicitly unset separately. An already contaminated
    // traditional dbus-daemon cannot be repaired through its update-only API.
    values.remove("WAYLAND_SOCKET");
    values.remove("AT_SPI_BUS_ADDRESS");
    values.remove("DBUS_SESSION_BUS_PID");
    Ok(values)
}

fn path(path: &Path) -> io::Result<String> {
    if !path.is_absolute() {
        return Err(io::Error::other("activation paths must be absolute"));
    }
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| io::Error::other("activation path is not UTF-8"))
}
