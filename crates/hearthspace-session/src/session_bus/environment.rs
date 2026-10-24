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
        values.insert("XDG_RUNTIME_DIR".into(), path(&paths.host_runtime_dir)?);
        values.insert(
            "WAYLAND_DISPLAY".into(),
            path(&paths.session_dir.join(&session.wayland_display))?,
        );
        values.insert(MARKER.into(), path(&paths.session_dir)?);
        // Absence in this launch must not retain another session's integration.
        for key in KEYS {
            if !matches!(*key, "WAYLAND_SOCKET" | "AT_SPI_BUS_ADDRESS") {
                values.entry((*key).into()).or_default();
            }
        }
    }
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
