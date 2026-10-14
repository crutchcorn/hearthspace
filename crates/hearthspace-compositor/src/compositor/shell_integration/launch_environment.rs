use std::{
    env, fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

use hearthspace_shell::app_catalog::DesktopApp;
#[cfg(feature = "test-apps")]
use {super::runtime_path, crate::config::*, std::process::Command, tracing::warn};

pub(super) fn host_runtime_dir() -> PathBuf {
    env::var_os(hearthspace_common::HOST_RUNTIME_DIR_ENV)
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from))
        .unwrap_or_else(env::temp_dir)
}

/// Snap instance names are embedded into a runtime directory path, so reject any
/// value containing path separators or other characters that could escape the
/// intended `snap.<name>` directory.
fn is_valid_snap_instance_name(instance_name: &str) -> bool {
    instance_name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

pub(super) fn snap_wayland_socket_path(
    host_runtime_dir: &Path,
    instance_name: &str,
    wayland_display: &str,
) -> PathBuf {
    host_runtime_dir
        .join(format!("snap.{instance_name}"))
        .join(wayland_display)
}

pub(super) fn ensure_snap_instance_name(instance_name: &str) -> std::io::Result<()> {
    if !is_valid_snap_instance_name(instance_name) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("invalid snap instance name {instance_name:?}"),
        ));
    }

    Ok(())
}

pub(super) fn launch_environment_for_app(
    app: &DesktopApp,
) -> std::io::Result<Vec<(String, String)>> {
    if !isolate_profile(&app.id, env::var("HEARTHSPACE_ISOLATE_APP_PROFILES").ok().as_deref()) {
        return Ok(Vec::new());
    }
    let base = app_state_base_dir().join(sanitized_path_component(&app.id));
    let config = base.join("config");
    let cache = base.join("cache");
    let data = base.join("data");
    let state = base.join("state");

    for dir in [&config, &cache, &data, &state] {
        fs::create_dir_all(dir)?;
    }

    Ok(vec![
        (
            "XDG_CONFIG_HOME".to_string(),
            config.to_string_lossy().into_owned(),
        ),
        (
            "XDG_CACHE_HOME".to_string(),
            cache.to_string_lossy().into_owned(),
        ),
        (
            "XDG_DATA_HOME".to_string(),
            data.to_string_lossy().into_owned(),
        ),
        (
            "XDG_STATE_HOME".to_string(),
            state.to_string_lossy().into_owned(),
        ),
    ])
}

fn isolate_profile(app_id: &str, preference: Option<&str>) -> bool {
    // Settings is a client of the session daemon and must display its actual
    // configuration path, even in a test session with isolated third-party apps.
    preference == Some("1") && app_id != "dev.hearthspace.Settings.desktop"
}

fn app_state_base_dir() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(env::temp_dir)
        .join("hearthspace/apps")
}

fn sanitized_path_component(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(feature = "test-apps")]
fn ensure_gtk_client_settings() -> std::io::Result<PathBuf> {
    let config_dir = runtime_path(GTK_CLIENT_CONFIG_DIR_NAME);
    let settings = "[Settings]\ngtk-decoration-layout=:close\n";

    for gtk_version_dir in ["gtk-3.0", "gtk-4.0"] {
        let dir = config_dir.join(gtk_version_dir);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("settings.ini"), settings)?;
    }

    let gsettings_dir = config_dir.join("glib-2.0/settings");
    fs::create_dir_all(&gsettings_dir)?;
    fs::write(
        gsettings_dir.join("keyfile"),
        "[org/gnome/desktop/wm/preferences]\nbutton-layout=':close'\n",
    )?;

    Ok(config_dir)
}

#[cfg(feature = "test-apps")]
pub(super) fn apply_gtk_client_environment(command: &mut Command) {
    match ensure_gtk_client_settings() {
        Ok(config_dir) => {
            command.env("XDG_CONFIG_HOME", config_dir);
            command.env("GSETTINGS_BACKEND", "keyfile");
        }
        Err(error) => warn!(%error, "failed to prepare GTK client settings"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_app_configuration_is_shared_unless_isolation_is_explicit() {
        assert!(!isolate_profile("org.gnome.TextEditor.desktop", None));
        assert!(!isolate_profile("org.gnome.TextEditor.desktop", Some("0")));
        assert!(isolate_profile("org.gnome.TextEditor.desktop", Some("1")));
        assert!(!isolate_profile("dev.hearthspace.Settings.desktop", Some("1")));
    }

    #[test]
    fn sanitized_path_component_replaces_unsafe_characters() {
        assert_eq!(
            sanitized_path_component("org.gnome.Calculator"),
            "org.gnome.Calculator"
        );
        // Path separators become underscores; dots are preserved.
        assert_eq!(sanitized_path_component("a/../b"), "a_.._b");
        assert_eq!(sanitized_path_component("weird name!"), "weird_name_");
    }

    #[test]
    fn snap_instance_names_reject_path_separators() {
        assert!(is_valid_snap_instance_name("firefox"));
        assert!(is_valid_snap_instance_name("foo_bar-1.2"));
        assert!(!is_valid_snap_instance_name("../escape"));
        assert!(!is_valid_snap_instance_name("with/slash"));
        assert!(!is_valid_snap_instance_name("space here"));
    }

    #[test]
    fn snap_wayland_socket_path_uses_host_snap_runtime() {
        let socket_path =
            snap_wayland_socket_path(Path::new("/run/user/1000"), "firefox", "wayland-99");

        assert_eq!(
            socket_path,
            PathBuf::from("/run/user/1000/snap.firefox/wayland-99")
        );
    }
}
