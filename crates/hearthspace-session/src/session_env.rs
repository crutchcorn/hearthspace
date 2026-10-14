use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process,
};

use crate::cli::{Args, RunMode};

#[derive(Debug)]
pub(crate) struct SessionPaths {
    pub(crate) host_runtime_dir: PathBuf,
    pub(crate) session_dir: PathBuf,
    pub(crate) applications_dir: PathBuf,
    pub(crate) settings_socket: PathBuf,
    pub(crate) session_socket: PathBuf,
    pub(crate) shell_command_socket: PathBuf,
}

#[derive(Debug)]
pub(crate) struct SessionEnv {
    pub(crate) wayland_display: String,
    pub(crate) vars: Vec<(String, String)>,
}

pub(crate) fn create_session_paths(
    requested: Option<&Path>,
) -> Result<SessionPaths, Box<dyn std::error::Error>> {
    let host_runtime_dir = env::var_os(hearthspace_common::HOST_RUNTIME_DIR_ENV)
        .or_else(|| env::var_os("XDG_RUNTIME_DIR"))
        .ok_or("XDG_RUNTIME_DIR is not set")?;
    let host_runtime_dir = PathBuf::from(host_runtime_dir);
    let session_dir = requested.map(Path::to_path_buf).unwrap_or_else(|| {
        host_runtime_dir
            .join("hearthspace")
            .join(format!("session-{}", process::id()))
    });
    // Never erase preexisting content during shutdown, even after startup fails.
    if session_dir.exists() && fs::read_dir(&session_dir)?.next().is_some() {
        return Err(format!("session directory must be empty: {}", session_dir.display()).into());
    }
    let applications_dir = session_dir.join("share/applications");

    fs::create_dir_all(&applications_dir)?;
    fs::set_permissions(&session_dir, fs::Permissions::from_mode(0o700))?;

    Ok(SessionPaths {
        host_runtime_dir,
        settings_socket: session_dir.join("settings.sock"),
        session_socket: session_dir.join("session.sock"),
        shell_command_socket: session_dir.join("shell-command.sock"),
        session_dir,
        applications_dir,
    })
}

pub(crate) fn create_session_env(
    args: &Args,
    mode: RunMode,
    paths: &SessionPaths,
) -> Result<SessionEnv, Box<dyn std::error::Error>> {
    let wayland_display = args
        .wayland_display
        .clone()
        .unwrap_or_else(|| hearthspace_common::WAYLAND_DISPLAY_NAME.to_string());
    let host_runtime_dir = path_to_string(&paths.host_runtime_dir)?;
    let session_dir = path_to_string(&paths.session_dir)?;
    let settings_socket = path_to_string(&paths.settings_socket)?;
    let session_socket = path_to_string(&paths.session_socket)?;
    let shell_command_socket = path_to_string(&paths.shell_command_socket)?;
    let mut vars = vec![
        ("XDG_RUNTIME_DIR".to_string(), session_dir.clone()),
        ("WAYLAND_DISPLAY".to_string(), wayland_display.clone()),
        (
            "XDG_CURRENT_DESKTOP".to_string(),
            hearthspace_common::CURRENT_DESKTOP.to_string(),
        ),
        (
            "DESKTOP_SESSION".to_string(),
            hearthspace_common::DESKTOP_SESSION.to_string(),
        ),
        (
            hearthspace_common::SESSION_DIR_ENV.to_string(),
            session_dir.clone(),
        ),
        (
            hearthspace_common::HOST_RUNTIME_DIR_ENV.to_string(),
            host_runtime_dir,
        ),
        (
            hearthspace_common::SETTINGS_SOCKET_ENV.to_string(),
            settings_socket,
        ),
        (
            hearthspace_common::SESSION_SOCKET_ENV.to_string(),
            session_socket,
        ),
        (
            hearthspace_common::SHELL_COMMAND_SOCKET_ENV.to_string(),
            shell_command_socket,
        ),
        ("RUST_LOG".to_string(), args.log_level.clone()),
        (
            "XDG_DATA_DIRS".to_string(),
            xdg_data_dirs(&path_to_string(&paths.session_dir.join("share"))?)?,
        ),
    ];

    if args.private_session_bus {
        let bus = env::var("DBUS_SESSION_BUS_ADDRESS")
            .map_err(|_| "--private-session-bus requires dbus-run-session")?;
        fs::write(paths.session_dir.join("session-bus.txt"), &bus)?;
        vars.push(("HEARTHSPACE_SESSION_BUS_ADDRESS".to_string(), bus));
    }

    if mode == RunMode::DevCargo {
        vars.push((hearthspace_common::DEV_ENV.to_string(), "1".to_string()));
        vars.push((
            hearthspace_common::REPO_ROOT_ENV.to_string(),
            path_to_string(&repo_root())?,
        ));
    }

    Ok(SessionEnv {
        wayland_display,
        vars,
    })
}

fn path_to_string(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(path
        .to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))?
        .to_string())
}

fn xdg_data_dirs(session_share_dir: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut dirs = vec![PathBuf::from(session_share_dir)];
    if let Some(existing) = env::var_os("XDG_DATA_DIRS") {
        dirs.extend(env::split_paths(&existing));
    } else {
        dirs.push(PathBuf::from("/usr/local/share"));
        dirs.push(PathBuf::from("/usr/share"));
    }
    Ok(env::join_paths(dirs)?.to_string_lossy().into_owned())
}

pub(crate) fn repo_root() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize().unwrap_or(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_env_uses_private_runtime_dir() {
        let paths = SessionPaths {
            host_runtime_dir: PathBuf::from("/tmp/hearthspace-test"),
            session_dir: PathBuf::from("/tmp/hearthspace-test/session-1"),
            applications_dir: PathBuf::from("/tmp/hearthspace-test/session-1/share/applications"),
            settings_socket: PathBuf::from("/tmp/hearthspace-test/session-1/settings.sock"),
            session_socket: PathBuf::from("/tmp/hearthspace-test/session-1/session.sock"),
            shell_command_socket: PathBuf::from(
                "/tmp/hearthspace-test/session-1/shell-command.sock",
            ),
        };
        let args = Args {
            backend: None,
            session_dir: None,
            binary_dir: None,
            compositor_binary: None,
            private_session_bus: false,
            startup_timeout: 120,
            dev: false,
            installed: true,
            wayland_display: Some("hearthspace-test".to_string()),
            watch_apps: false,
            no_watch_apps: false,
            no_shell_restart: false,
            no_settingsd_restart: false,
            log_level: "info".to_string(),
            print_env: false,
            dry_run: false,
        };

        let env = create_session_env(&args, RunMode::InstalledBinaries, &paths).unwrap();

        assert!(env.vars.iter().any(|(key, value)| {
            key == "XDG_RUNTIME_DIR" && value == "/tmp/hearthspace-test/session-1"
        }));
    }

    #[test]
    fn session_env_uses_stable_default_wayland_display() {
        let paths = SessionPaths {
            host_runtime_dir: PathBuf::from("/tmp/hearthspace-test"),
            session_dir: PathBuf::from("/tmp/hearthspace-test/session-1"),
            applications_dir: PathBuf::from("/tmp/hearthspace-test/session-1/share/applications"),
            settings_socket: PathBuf::from("/tmp/hearthspace-test/session-1/settings.sock"),
            session_socket: PathBuf::from("/tmp/hearthspace-test/session-1/session.sock"),
            shell_command_socket: PathBuf::from(
                "/tmp/hearthspace-test/session-1/shell-command.sock",
            ),
        };
        let args = Args {
            backend: None,
            session_dir: None,
            binary_dir: None,
            compositor_binary: None,
            private_session_bus: false,
            startup_timeout: 120,
            dev: false,
            installed: true,
            wayland_display: None,
            watch_apps: false,
            no_watch_apps: false,
            no_shell_restart: false,
            no_settingsd_restart: false,
            log_level: "info".to_string(),
            print_env: false,
            dry_run: false,
        };

        let env = create_session_env(&args, RunMode::InstalledBinaries, &paths).unwrap();

        assert_eq!(
            env.wayland_display,
            hearthspace_common::WAYLAND_DISPLAY_NAME
        );
        assert!(env.vars.iter().any(|(key, value)| {
            key == hearthspace_common::HOST_RUNTIME_DIR_ENV && value == "/tmp/hearthspace-test"
        }));
    }
}
