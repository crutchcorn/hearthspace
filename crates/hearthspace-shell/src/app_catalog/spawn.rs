use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

const SETTINGS_APP_BINARY: &str = "hearthspace-settings";

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
        .env_remove("DISPLAY")
        .env_remove("DBUS_SESSION_BUS_ADDRESS");
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
    if program != SETTINGS_APP_BINARY || path.components().count() != 1 {
        return path.to_path_buf();
    }

    env::current_exe()
        .ok()
        .and_then(|current_exe| current_exe.parent().map(|parent| parent.join(program)))
        .filter(|candidate| candidate.exists())
        .unwrap_or_else(|| path.to_path_buf())
}
