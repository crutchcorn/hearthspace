use crate::{
    settings_wire::{encode_background_path, validate_monitor_id},
    *,
};
use std::{
    env, io,
    path::{Path, PathBuf},
    time::Duration,
};

pub fn settings_socket_path() -> io::Result<PathBuf> {
    if let Some(socket_path) = env::var_os(hearthspace_common::SETTINGS_SOCKET_ENV) {
        return Ok(PathBuf::from(socket_path));
    }

    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set"))?;
    Ok(PathBuf::from(runtime_dir).join("hearthspace-settingsd.sock"))
}

pub fn client_get_settings() -> io::Result<Settings> {
    client_get_settings_at(&settings_socket_path()?)
}

/// Read a specific session's settings without changing process environment.
pub fn client_get_settings_at(path: &Path) -> io::Result<Settings> {
    // Native refresh runs on the compositor thread: an unavailable daemon must
    // fail soft without a long input/rendering stall.
    let response = transport::request_line(path, "get\n", Duration::from_millis(50))?;
    parse_settings_response(&response)
}

pub fn client_set_test(test: bool) -> io::Result<Settings> {
    let value = if test { "true" } else { "false" };
    send_settings_request(&format!("set test {value}\n"))
}

pub fn client_set_color_mode(mode: ColorMode) -> io::Result<Settings> {
    send_settings_request(&format!("set color-mode {mode}\n"))
}

pub fn client_set_accent_color(accent: AccentColor) -> io::Result<Settings> {
    send_settings_request(&format!("set accent-color {accent}\n"))
}

pub fn client_set_background_mode(mode: BackgroundMode) -> io::Result<Settings> {
    send_settings_request(&format!("set background-mode {mode}\n"))
}

pub fn client_set_background_image_path(path: Option<&str>) -> io::Result<Settings> {
    let path = path
        .map(encode_background_path)
        .unwrap_or_else(|| "default".to_string());
    send_settings_request(&format!("set background-image-path {path}\n"))
}

pub fn client_set_monitor_position(id: &str, x: i32, y: i32) -> io::Result<Settings> {
    validate_monitor_id(id)?;
    send_settings_request(&format!("set monitor {id} {x} {y}\n"))
}

pub fn client_set_monitor_scale(id: &str, scale: i32) -> io::Result<Settings> {
    validate_monitor_id(id)?;
    if scale <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("monitor scale must be positive: {scale}"),
        ));
    }
    send_settings_request(&format!("set monitor-scale {id} {scale}\n"))
}

fn send_settings_request(request: &str) -> io::Result<Settings> {
    send_settings_request_with_timeout(request, Duration::from_millis(1000))
}

fn send_settings_request_with_timeout(request: &str, timeout: Duration) -> io::Result<Settings> {
    let response = transport::request_line(&settings_socket_path()?, request, timeout)?;
    parse_settings_response(&response)
}
