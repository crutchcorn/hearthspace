use std::{
    env,
    path::{Path, PathBuf},
    process::{Child, Command},
};

use tracing::warn;

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
    spawn_argv_with_options(argv, wayland_display, envs, None)
}

pub fn spawn_argv_with_options(
    argv: &[String],
    wayland_display: &str,
    envs: &[(&str, &str)],
    working_directory: Option<&Path>,
) -> Result<(), String> {
    let mut command = launch_command(argv, wayland_display, envs, working_directory)?;
    let child = command
        .spawn()
        .map_err(|error| format!("failed to spawn {}: {error}", argv[0]))?;
    reap_child(child);
    Ok(())
}

pub(crate) fn launch_command(
    argv: &[String],
    wayland_display: &str,
    envs: &[(&str, &str)],
    working_directory: Option<&Path>,
) -> Result<Command, String> {
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
    if let Some(directory) = working_directory {
        command.current_dir(directory);
    }
    Ok(command)
}

/// Wait only for the child we spawned; never consume XWayland/supervisor exits.
pub fn reap_child(mut child: Child) -> u32 {
    let pid = child.id();
    std::thread::spawn(move || {
        if let Err(error) = child.wait() {
            warn!(pid, %error, "failed to reap launched application");
        }
    });
    pid
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

    #[test]
    fn launched_program_uses_desktop_working_directory() {
        let directory = tempfile::tempdir().unwrap();
        let working = directory.path().join("project with spaces");
        std::fs::create_dir(&working).unwrap();
        let mut command =
            launch_command(&["/bin/pwd".into()], "wayland-test", &[], Some(&working)).unwrap();
        let output = command.output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim_end(),
            working.to_str().unwrap()
        );
        assert!(
            spawn_argv_with_options(
                &["/bin/true".into()],
                "wayland-test",
                &[],
                Some(&working.join("missing"))
            )
            .is_err()
        );
    }

    #[test]
    fn exited_child_is_reaped_without_waiting_for_other_processes() {
        let child = Command::new("/bin/true").spawn().unwrap();
        let pid = reap_child(child);
        let process = PathBuf::from(format!("/proc/{pid}"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while process.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!process.exists(), "exited child was left as a zombie");
    }
}
