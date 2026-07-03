pub const WAYLAND_DISPLAY_NAME: &str = "wayland-99";
pub const WAYLAND_DISPLAY_ENV: &str = "WAYLAND_DISPLAY";
pub const HEARTHSPACE_WAYLAND_DISPLAY_ENV: &str = "HEARTHSPACE_WAYLAND_DISPLAY";
pub const SHELL_COMMAND_SOCKET_NAME: &str = "hearthspace-shell.sock";
pub const SHELL_COMMAND_SOCKET_ENV: &str = "HEARTHSPACE_COMMAND_SOCKET";
pub const LOG_FILE_ENV: &str = "HEARTHSPACE_LOG_FILE";
pub const SETTINGS_SOCKET_ENV: &str = "HEARTHSPACE_SETTINGS_SOCKET";
pub const SESSION_SOCKET_ENV: &str = "HEARTHSPACE_SESSION_SOCKET";
pub const SESSION_DIR_ENV: &str = "HEARTHSPACE_SESSION_DIR";
pub const REPO_ROOT_ENV: &str = "HEARTHSPACE_REPO_ROOT";
pub const DEV_ENV: &str = "HEARTHSPACE_DEV";
pub const CURRENT_DESKTOP: &str = "Hearthspace";
pub const DESKTOP_SESSION: &str = "hearthspace";
const CONFIG_DIR_NAME: &str = "hearthspace";
const CONFIG_FILE_NAME: &str = "config.toml";

pub fn config_path() -> std::io::Result<std::path::PathBuf> {
    Ok(config_home()?.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME))
}

fn config_home() -> std::io::Result<std::path::PathBuf> {
    if let Some(xdg_config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(std::path::PathBuf::from(xdg_config_home));
    }

    let home = std::env::var_os("HOME").ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "neither XDG_CONFIG_HOME nor HOME is set",
        )
    })?;
    Ok(std::path::PathBuf::from(home).join(".config"))
}

pub fn wayland_display_name() -> String {
    std::env::var(HEARTHSPACE_WAYLAND_DISPLAY_ENV)
        .or_else(|_| std::env::var(WAYLAND_DISPLAY_ENV))
        .unwrap_or_else(|_| WAYLAND_DISPLAY_NAME.to_string())
}
