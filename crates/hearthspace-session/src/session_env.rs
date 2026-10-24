use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process,
};

use crate::cli::{Args, RunMode};

mod bus;

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
    private_session_bus: bool,
) -> Result<SessionPaths, Box<dyn std::error::Error>> {
    let (host_runtime_dir, session_dir) = runtime_directories(requested)?;
    prepare_session_directory(&session_dir, private_session_bus)?;
    let applications_dir = session_dir.join("share/applications");
    fs::create_dir_all(&applications_dir)?;

    Ok(SessionPaths {
        host_runtime_dir,
        settings_socket: session_dir.join("settings.sock"),
        session_socket: session_dir.join("session.sock"),
        shell_command_socket: session_dir.join("shell-command.sock"),
        session_dir,
        applications_dir,
    })
}

pub(crate) fn runtime_directories(
    requested: Option<&Path>,
) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    let host_runtime_dir = env::var_os(hearthspace_common::HOST_RUNTIME_DIR_ENV)
        .or_else(|| env::var_os("XDG_RUNTIME_DIR"))
        .ok_or("XDG_RUNTIME_DIR is not set")?;
    let host_runtime_dir = PathBuf::from(host_runtime_dir);
    let session_dir = requested.map(Path::to_path_buf).unwrap_or_else(|| {
        host_runtime_dir
            .join("hearthspace")
            .join(format!("session-{}", process::id()))
    });
    if !host_runtime_dir.is_absolute() || !session_dir.is_absolute() {
        return Err("host and session runtime directories must be absolute paths".into());
    }
    Ok((host_runtime_dir, session_dir))
}

pub(crate) fn prepare_session_directory(
    session_dir: &Path,
    private_session_bus: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // dbus-run-session creates dbus-1/services before executing its child.
    // Only its explicitly requested private bus directory may preexist.
    if session_dir.exists() {
        for entry in fs::read_dir(session_dir)? {
            let entry = entry?;
            if !(private_session_bus
                && entry.file_name() == "dbus-1"
                && entry.file_type()?.is_dir())
            {
                return Err(
                    format!("session directory must be empty: {}", session_dir.display()).into(),
                );
            }
        }
    }
    fs::create_dir_all(session_dir)?;
    fs::set_permissions(session_dir, fs::Permissions::from_mode(0o700))?;
    Ok(())
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
        ("XDG_SESSION_TYPE".to_string(), "wayland".to_string()),
        (
            "XDG_SESSION_DESKTOP".to_string(),
            hearthspace_common::CURRENT_DESKTOP.to_string(),
        ),
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
            host_runtime_dir.clone(),
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

    let private_clients = args.private_session_bus || !crate::compositor::is_native_session(args);
    let (app_runtime, app_display) = hearthspace_desktop::environment::connection(
        &paths.host_runtime_dir,
        &paths.session_dir,
        &wayland_display,
        private_clients,
    )?;
    vars.extend([
        (
            hearthspace_desktop::environment::APP_RUNTIME.into(),
            app_runtime,
        ),
        (
            hearthspace_desktop::environment::APP_DISPLAY.into(),
            app_display,
        ),
    ]);
    vars.extend(crate::audio_env::audio_environment(&host_runtime_dir)?);
    vars.push((
        "XDG_CONFIG_DIRS".into(),
        crate::gtk_defaults::config_dirs(&paths.session_dir)?,
    ));

    if let Some(bus) = bus::selected_address(args, paths)? {
        let generation = fs::read_to_string("/proc/sys/kernel/random/uuid")?;
        hearthspace_desktop::activation::initialize_display(
            &paths.host_runtime_dir,
            &paths.session_dir,
            &bus,
            args.private_session_bus,
            generation.trim(),
        )?;
        vars.push((
            hearthspace_desktop::activation::MARKER.into(),
            generation.trim().into(),
        ));
        // Direct and activated launches use the same selected bus. The adapter
        // keeps terminal scopes owned by this session on either bus.
        vars.push((
            "PATH".into(),
            crate::app_scopes::command_path(&paths.session_dir, env::var_os("PATH"))?,
        ));
        fs::write(paths.session_dir.join("session-bus.txt"), &bus)?;
        vars.push(("DBUS_SESSION_BUS_ADDRESS".into(), bus.clone()));
        vars.push(("HEARTHSPACE_SESSION_BUS_ADDRESS".into(), bus));
    }
    if crate::desktop_services::enabled(args) {
        // GTK 3 Wayland otherwise reads the host's saved GNOME preferences.
        // The portal publishes this session's choices without modifying them.
        vars.push(("GTK_USE_PORTAL".into(), "1".into()));
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
mod tests;
