//! Desktop defaults stay in the owned runtime; user preferences remain authoritative.
use std::{
    env,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};

use crate::session_env::SessionEnv;

mod schema;
#[cfg(test)]
mod tests;

const GTK_SETTINGS: &str = "[Settings]\ngtk-decoration-layout=:maximize,close\n";

pub(crate) fn config_dirs(runtime: &Path) -> io::Result<String> {
    merge_config_dirs(runtime, env::var_os("XDG_CONFIG_DIRS").as_deref())
}

fn merge_config_dirs(runtime: &Path, existing: Option<&OsStr>) -> io::Result<String> {
    let mut dirs = vec![runtime.join("desktop-config")];
    let existing = existing.filter(|value| !value.is_empty());
    dirs.extend(existing.map_or_else(
        || vec![PathBuf::from("/etc/xdg")],
        |value| env::split_paths(value).collect(),
    ));
    // GTK reads these directories in order, then XDG_CONFIG_HOME. Put our
    // fallback first so existing administrator/user settings.ini values win.
    env::join_paths(dirs)
        .map_err(io::Error::other)?
        .into_string()
        .map_err(|_| io::Error::other("GTK configuration paths must be valid UTF-8"))
}

pub(crate) fn prepare(runtime: &Path, session: &SessionEnv) {
    let data_dirs = session
        .vars
        .iter()
        .find(|(key, _)| key == "XDG_DATA_DIRS")
        .map(|(_, value)| value.as_str())
        .unwrap_or("/usr/local/share:/usr/share");
    if let Err(error) = write_defaults(runtime, data_dirs) {
        tracing::warn!(%error, "GTK window controls could not receive Hearthspace defaults");
    }
}

fn write_defaults(runtime: &Path, data_dirs: &str) -> io::Result<()> {
    for version in ["gtk-3.0", "gtk-4.0"] {
        let directory = runtime.join("desktop-config").join(version);
        fs::create_dir_all(&directory)?;
        fs::write(directory.join("settings.ini"), GTK_SETTINGS)?;
    }
    // Wayland GTK reads this value from GSettings above settings.ini. A schema
    // default changes neither the user's dconf/keyfile backend nor its values.
    // XDG_DATA_DIRS already includes runtime/share for every session child and
    // private D-Bus activation. An explicit GSETTINGS_SCHEMA_DIR stays untouched.
    schema::prepare(runtime, data_dirs)
}
