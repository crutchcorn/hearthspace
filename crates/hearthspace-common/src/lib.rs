mod appearance;
mod desktop;
mod shell_corners;

pub use appearance::{AccentColor, Appearance, ColorMode};
pub use desktop::{
    DESKTOP_ITEM_APP_ID, DESKTOP_ITEM_HEIGHT, DESKTOP_ITEM_MARGIN, DESKTOP_ITEM_WIDTH,
    DESKTOP_SURFACE_HEIGHT, DESKTOP_SURFACE_WIDTH, MAX_DESKTOP_ITEMS, desktop_item_app_id,
    desktop_item_id_from_app_id,
};

pub use shell_corners::{
    NOTIFICATION_CORNER_WIDTH, SHELL_CORNER_CLEARANCE, SHELL_CORNER_MARGIN, SHELL_CORNER_RADIUS,
    ShellCorner, shell_corner_app_id, shell_corner_from_app_id,
};

pub const WAYLAND_DISPLAY_NAME: &str = "wayland-99";
pub const WAYLAND_DISPLAY_ENV: &str = "WAYLAND_DISPLAY";
pub const HEARTHSPACE_WAYLAND_DISPLAY_ENV: &str = "HEARTHSPACE_WAYLAND_DISPLAY";
pub const SHELL_COMMAND_SOCKET_NAME: &str = "hearthspace-shell.sock";
pub const SHELL_COMMAND_SOCKET_ENV: &str = "HEARTHSPACE_COMMAND_SOCKET";
pub const SHELL_BAR_APP_ID: &str = "dev.hearthspace.shell-bar";
pub const LAUNCHER_APP_ID: &str = "dev.hearthspace.launcher";
pub const CANVAS_MAP_APP_ID: &str = "dev.hearthspace.canvas-map";
pub const NOTIFICATIONS_APP_ID: &str = "dev.hearthspace.notifications-panel";
pub const QUICK_PANEL_APP_ID: &str = "dev.hearthspace.quick-panel";
pub const CONTROL_OSD_APP_ID: &str = "dev.hearthspace.control-osd";
pub const POWER_MENU_APP_ID: &str = "dev.hearthspace.power-menu";
pub const TRAY_MENU_APP_ID: &str = "dev.hearthspace.tray-menu";
pub const SHELL_OUTPUT_WATCHER_APP_ID: &str = "dev.hearthspace.shell-output-watcher";
pub const CONTROL_BAR_HEIGHT: i32 = 54;
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

pub fn launcher_app_id_for_request(output_name: Option<&str>, token: u64) -> String {
    let base = output_name
        .map(launcher_app_id_for_output)
        .unwrap_or_else(|| LAUNCHER_APP_ID.to_string());
    format!("{base}.request-{token}")
}

pub fn launcher_request_token_from_app_id(app_id: &str) -> Option<u64> {
    let (base, token) = app_id.rsplit_once(".request-")?;
    shell_output_name_from_app_id(LAUNCHER_APP_ID, base)?;
    token.parse::<u64>().ok().filter(|token| *token > 0)
}

pub fn canvas_map_app_id_for_output(output_name: &str) -> String {
    shell_app_id_for_output(CANVAS_MAP_APP_ID, output_name)
}

pub fn notifications_app_id_for_output(output_name: &str) -> String {
    shell_app_id_for_output(NOTIFICATIONS_APP_ID, output_name)
}

pub fn power_menu_app_id_for_output(output_name: &str) -> String {
    shell_app_id_for_output(POWER_MENU_APP_ID, output_name)
}

pub fn quick_panel_app_id_for_output(output_name: &str) -> String {
    shell_app_id_for_output(QUICK_PANEL_APP_ID, output_name)
}

pub fn control_osd_app_id_for_output(output_name: &str) -> String {
    shell_app_id_for_output(CONTROL_OSD_APP_ID, output_name)
}

pub fn tray_menu_app_id_for_output(output_name: &str) -> String {
    shell_app_id_for_output(TRAY_MENU_APP_ID, output_name)
}

/// Private compositor-to-shell datagrams share the command socket's namespace.
pub fn shell_event_socket_path(command_socket: &std::path::Path) -> std::path::PathBuf {
    let mut path = command_socket.as_os_str().to_os_string();
    path.push(".events");
    path.into()
}

pub fn shell_bar_output_name_from_app_id(app_id: &str) -> Option<Option<String>> {
    shell_output_name_from_app_id(SHELL_BAR_APP_ID, app_id)
        .or_else(|| shell_corners::output_name_from_app_id(app_id))
}

pub fn launcher_output_name_from_app_id(app_id: &str) -> Option<Option<String>> {
    let base = if let Some((base, _)) = app_id.rsplit_once(".request-") {
        launcher_request_token_from_app_id(app_id)?;
        base
    } else {
        app_id
    };
    shell_output_name_from_app_id(LAUNCHER_APP_ID, base)
}

pub fn canvas_map_output_name_from_app_id(app_id: &str) -> Option<Option<String>> {
    shell_output_name_from_app_id(CANVAS_MAP_APP_ID, app_id)
}

pub fn notifications_output_name_from_app_id(app_id: &str) -> Option<Option<String>> {
    shell_output_name_from_app_id(NOTIFICATIONS_APP_ID, app_id)
}

pub fn power_menu_output_name_from_app_id(app_id: &str) -> Option<Option<String>> {
    shell_output_name_from_app_id(POWER_MENU_APP_ID, app_id)
}

pub fn quick_panel_output_name_from_app_id(app_id: &str) -> Option<Option<String>> {
    shell_output_name_from_app_id(QUICK_PANEL_APP_ID, app_id)
}

pub fn control_osd_output_name_from_app_id(app_id: &str) -> Option<Option<String>> {
    shell_output_name_from_app_id(CONTROL_OSD_APP_ID, app_id)
}

pub fn tray_menu_output_name_from_app_id(app_id: &str) -> Option<Option<String>> {
    shell_output_name_from_app_id(TRAY_MENU_APP_ID, app_id)
}

pub fn shell_panel_output_name_from_app_id(app_id: &str) -> Option<Option<String>> {
    launcher_output_name_from_app_id(app_id)
        .or_else(|| canvas_map_output_name_from_app_id(app_id))
        .or_else(|| notifications_output_name_from_app_id(app_id))
        .or_else(|| power_menu_output_name_from_app_id(app_id))
        .or_else(|| tray_menu_output_name_from_app_id(app_id))
        .or_else(|| quick_panel_output_name_from_app_id(app_id))
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
        assert_eq!(
            canvas_map_output_name_from_app_id(CANVAS_MAP_APP_ID),
            Some(None)
        );
    }

    #[test]
    fn canvas_map_app_id_round_trips_and_rejects_unrelated_or_invalid_ids() {
        let app_id = canvas_map_app_id_for_output("card0/Virtual Display");
        assert_eq!(
            canvas_map_output_name_from_app_id(&app_id),
            Some(Some("card0/Virtual Display".to_string()))
        );
        assert_eq!(canvas_map_output_name_from_app_id(LAUNCHER_APP_ID), None);
        assert_eq!(
            canvas_map_output_name_from_app_id("dev.hearthspace.canvas-map.output-ff"),
            None
        );
    }

    #[test]
    fn invalid_shell_app_id_suffix_is_rejected() {
        assert_eq!(
            shell_bar_output_name_from_app_id("dev.hearthspace.shell-bar.output-not-hex"),
            None
        );
    }

    #[test]
    fn every_shell_panel_has_a_distinct_output_namespace() {
        let output = "card0/DP-1 🖥";
        for (base, app_id) in [
            (LAUNCHER_APP_ID, launcher_app_id_for_output(output)),
            (CANVAS_MAP_APP_ID, canvas_map_app_id_for_output(output)),
            (
                NOTIFICATIONS_APP_ID,
                notifications_app_id_for_output(output),
            ),
            (POWER_MENU_APP_ID, power_menu_app_id_for_output(output)),
            (TRAY_MENU_APP_ID, tray_menu_app_id_for_output(output)),
            (QUICK_PANEL_APP_ID, quick_panel_app_id_for_output(output)),
        ] {
            assert_eq!(shell_panel_output_name_from_app_id(base), Some(None));
            assert_eq!(
                shell_panel_output_name_from_app_id(&app_id),
                Some(Some(output.into()))
            );
            assert_eq!(shell_bar_output_name_from_app_id(&app_id), None);
            assert_eq!(
                shell_panel_output_name_from_app_id(&format!("{base}.output-ff")),
                None
            );
        }
        assert_eq!(shell_panel_output_name_from_app_id(SHELL_BAR_APP_ID), None);
        assert_eq!(
            shell_event_socket_path(std::path::Path::new("/tmp/control.sock")),
            std::path::PathBuf::from("/tmp/control.sock.events")
        );
    }

    #[test]
    fn launcher_request_ids_preserve_output_and_reject_bad_tokens() {
        for output in [None, Some("card0/DP-1")] {
            let id = launcher_app_id_for_request(output, 42);
            assert_eq!(
                launcher_output_name_from_app_id(&id),
                Some(output.map(str::to_owned))
            );
            assert_eq!(launcher_request_token_from_app_id(&id), Some(42));
        }
        assert_eq!(launcher_request_token_from_app_id(LAUNCHER_APP_ID), None);
        for suffix in ["0", "-1", "a", "18446744073709551616", "1.request-2"] {
            assert_eq!(
                launcher_output_name_from_app_id(&format!("{LAUNCHER_APP_ID}.request-{suffix}")),
                None
            );
        }
    }
    #[test]
    fn osd_output_names_round_trip_without_becoming_interactive_panels() {
        let id = control_osd_app_id_for_output("card0/DP-1 🖥");
        assert_eq!(
            control_osd_output_name_from_app_id(&id),
            Some(Some("card0/DP-1 🖥".into()))
        );
        assert_eq!(
            control_osd_output_name_from_app_id(CONTROL_OSD_APP_ID),
            Some(None)
        );
        assert_eq!(shell_panel_output_name_from_app_id(&id), None);
        assert_eq!(
            control_osd_output_name_from_app_id("dev.hearthspace.control-osd.output-ff"),
            None
        );
    }
}
