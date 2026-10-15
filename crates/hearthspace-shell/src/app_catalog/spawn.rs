use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

const SETTINGS_APP_BINARY: &str = "hearthspace-settings";
const UI_STORYBOOK_APP_BINARY: &str = "hearthspace-ui-storybook";

pub fn spawn_argv(argv: &[String]) -> Result<(), String> {
    let wayland_display = hearthspace_common::wayland_display_name();
    spawn_argv_with_wayland_display(argv, &wayland_display)
}

pub fn spawn_argv_with_wayland_display(
    argv: &[String],
    wayland_display: &str,
) -> Result<(), String> {
    spawn_argv_with_env(argv, wayland_display, &[])
}

pub fn spawn_argv_with_env(
    argv: &[String],
    wayland_display: &str,
    envs: &[(&str, &str)],
) -> Result<(), String> {
    let Some((program, args)) = argv.split_first() else {
        return Err("cannot spawn an empty command".to_string());
    };
    let mut command = Command::new(resolve_launch_program(program));
    command
        .args(args)
        .env("WAYLAND_DISPLAY", wayland_display)
        .env_remove("DISPLAY");
    configure_session_bus(&mut command, env::var_os("HEARTHSPACE_SESSION_BUS_ADDRESS"));
    for key in session_environment_keys() {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
    }
    for (key, value) in envs {
        command.env(key, value);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("failed to spawn {program}: {error}"))
}

fn configure_session_bus(command: &mut Command, private_bus: Option<std::ffi::OsString>) {
    // Host portal/accessibility services belong to the host desktop. Only the
    // supervisor's explicit private-session bus may be inherited by launched apps.
    command
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .env_remove("DBUS_SESSION_BUS_PID");
    if let Some(address) = private_bus {
        command.env("DBUS_SESSION_BUS_ADDRESS", address);
    }
}

pub(super) fn session_environment_keys() -> &'static [&'static str] {
    &[
        "XDG_CURRENT_DESKTOP",
        "DESKTOP_SESSION",
        hearthspace_common::SHELL_COMMAND_SOCKET_ENV,
        hearthspace_common::SETTINGS_SOCKET_ENV,
        hearthspace_common::SESSION_SOCKET_ENV,
        hearthspace_common::REPO_ROOT_ENV,
    ]
}

fn resolve_launch_program(program: &str) -> PathBuf {
    let path = Path::new(program);
    if !matches!(program, SETTINGS_APP_BINARY | UI_STORYBOOK_APP_BINARY)
        || path.components().count() != 1
    {
        return path.to_path_buf();
    }

    env::current_exe()
        .ok()
        .and_then(|current_exe| current_exe.parent().map(|parent| parent.join(program)))
        .filter(|candidate| candidate.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn apps_use_only_an_explicit_private_bus() {
        let mut command = Command::new("unused");
        configure_session_bus(&mut command, None);
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == "DBUS_SESSION_BUS_ADDRESS" && value.is_none())
        );
        configure_session_bus(&mut command, Some("unix:path=/private/bus".into()));
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == "DBUS_SESSION_BUS_ADDRESS"
                    && value == Some(OsStr::new("unix:path=/private/bus")))
        );
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == "DBUS_SESSION_BUS_PID" && value.is_none())
        );
    }
}
