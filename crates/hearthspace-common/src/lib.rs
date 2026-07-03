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

pub fn wayland_display_name() -> String {
    std::env::var(HEARTHSPACE_WAYLAND_DISPLAY_ENV)
        .or_else(|_| std::env::var(WAYLAND_DISPLAY_ENV))
        .unwrap_or_else(|_| WAYLAND_DISPLAY_NAME.to_string())
}
