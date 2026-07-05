use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process,
};

use crate::cli::{Args, RunMode};

#[derive(Debug)]
pub(crate) struct SessionPaths {
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

pub(crate) fn create_session_paths() -> Result<SessionPaths, Box<dyn std::error::Error>> {
    let host_runtime_dir = env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is not set")?;
    let session_dir = PathBuf::from(host_runtime_dir)
        .join("hearthspace")
        .join(format!("session-{}", process::id()));
    let applications_dir = session_dir.join("share/applications");

    fs::create_dir_all(&applications_dir)?;
    fs::set_permissions(&session_dir, fs::Permissions::from_mode(0o700))?;

    Ok(SessionPaths {
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
        .unwrap_or_else(|| format!("hearthspace-dev-{}", process::id()));
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
            session_dir: PathBuf::from("/tmp/hearthspace-test/session-1"),
            applications_dir: PathBuf::from("/tmp/hearthspace-test/session-1/share/applications"),
            settings_socket: PathBuf::from("/tmp/hearthspace-test/session-1/settings.sock"),
            session_socket: PathBuf::from("/tmp/hearthspace-test/session-1/session.sock"),
            shell_command_socket: PathBuf::from(
                "/tmp/hearthspace-test/session-1/shell-command.sock",
            ),
        };
        let args = Args {
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
}
