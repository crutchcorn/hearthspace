pub const WAYLAND_DISPLAY_NAME: &str = "wayland-99";
pub const WAYLAND_DISPLAY_ENV: &str = "WAYLAND_DISPLAY";
pub const HEARTHSPACE_WAYLAND_DISPLAY_ENV: &str = "HEARTHSPACE_WAYLAND_DISPLAY";
pub const SHELL_COMMAND_SOCKET_NAME: &str = "hearthspace-shell.sock";
pub const SHELL_COMMAND_SOCKET_ENV: &str = "HEARTHSPACE_COMMAND_SOCKET";
pub const SHELL_BAR_APP_ID: &str = "dev.hearthspace.shell-bar";
pub const LAUNCHER_APP_ID: &str = "dev.hearthspace.launcher";
pub const SHELL_OUTPUT_WATCHER_APP_ID: &str = "dev.hearthspace.shell-output-watcher";
pub const CONTROL_BAR_HEIGHT: i32 = 48;
pub const LOG_FILE_ENV: &str = "HEARTHSPACE_LOG_FILE";
pub const SETTINGS_SOCKET_ENV: &str = "HEARTHSPACE_SETTINGS_SOCKET";
pub const SESSION_SOCKET_ENV: &str = "HEARTHSPACE_SESSION_SOCKET";
pub const SESSION_DIR_ENV: &str = "HEARTHSPACE_SESSION_DIR";
pub const HOST_RUNTIME_DIR_ENV: &str = "HEARTHSPACE_HOST_XDG_RUNTIME_DIR";
pub const REPO_ROOT_ENV: &str = "HEARTHSPACE_REPO_ROOT";
pub const DEV_ENV: &str = "HEARTHSPACE_DEV";
pub const CURRENT_DESKTOP: &str = "Hearthspace";
pub const DESKTOP_SESSION: &str = "hearthspace";
pub const NO_SHELL_FLAG: &str = "--no-shell";
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

pub fn shell_bar_app_id_for_output(output_name: &str) -> String {
    shell_app_id_for_output(SHELL_BAR_APP_ID, output_name)
}

pub fn launcher_app_id_for_output(output_name: &str) -> String {
    shell_app_id_for_output(LAUNCHER_APP_ID, output_name)
}

pub fn shell_bar_output_name_from_app_id(app_id: &str) -> Option<Option<String>> {
    shell_output_name_from_app_id(SHELL_BAR_APP_ID, app_id)
}

pub fn launcher_output_name_from_app_id(app_id: &str) -> Option<Option<String>> {
    shell_output_name_from_app_id(LAUNCHER_APP_ID, app_id)
}

fn shell_app_id_for_output(base: &str, output_name: &str) -> String {
    format!("{base}.output-{}", hex_encode(output_name.as_bytes()))
}

fn shell_output_name_from_app_id(base: &str, app_id: &str) -> Option<Option<String>> {
    if app_id == base {
        return Some(None);
    }

    let suffix = app_id.strip_prefix(base)?.strip_prefix(".output-")?;
    let bytes = hex_decode(suffix)?;
    String::from_utf8(bytes).ok().map(Some)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn hex_decode(input: &str) -> Option<Vec<u8>> {
    if !input.len().is_multiple_of(2) {
        return None;
    }

    input
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Some(hex_value(pair[0])? << 4 | hex_value(pair[1])?))
        .collect()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_bar_app_id_round_trips_output_name() {
        let app_id = shell_bar_app_id_for_output("HDMI-A-1");

        assert_eq!(
            shell_bar_output_name_from_app_id(&app_id),
            Some(Some("HDMI-A-1".to_string()))
        );
    }

    #[test]
    fn launcher_app_id_round_trips_unusual_output_name() {
        let app_id = launcher_app_id_for_output("card0/Virtual Display");

        assert_eq!(
            launcher_output_name_from_app_id(&app_id),
            Some(Some("card0/Virtual Display".to_string()))
        );
    }

    #[test]
    fn base_shell_app_ids_map_to_primary_fallback() {
        assert_eq!(
            shell_bar_output_name_from_app_id(SHELL_BAR_APP_ID),
            Some(None)
        );
        assert_eq!(
            launcher_output_name_from_app_id(LAUNCHER_APP_ID),
            Some(None)
        );
    }

    #[test]
    fn invalid_shell_app_id_suffix_is_rejected() {
        assert_eq!(
            shell_bar_output_name_from_app_id("dev.hearthspace.shell-bar.output-not-hex"),
            None
        );
    }
}
