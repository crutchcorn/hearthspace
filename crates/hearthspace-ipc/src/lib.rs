use std::{
    env, io,
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub test: bool,
    pub monitors: Vec<MonitorConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorConfig {
    pub id: String,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub scale: i32,
    pub x: i32,
    pub y: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            test: false,
            monitors: vec![
                MonitorConfig {
                    id: "primary".to_string(),
                    name: "Primary".to_string(),
                    width: 1920,
                    height: 1080,
                    scale: 1,
                    x: 0,
                    y: 0,
                },
                MonitorConfig {
                    id: "secondary".to_string(),
                    name: "Secondary".to_string(),
                    width: 1920,
                    height: 1080,
                    scale: 1,
                    x: 1920,
                    y: 0,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsRequest {
    Get,
    SetTest(bool),
    SetMonitorPosition { id: String, x: i32, y: i32 },
    SetMonitorScale { id: String, scale: i32 },
}

pub fn settings_socket_path() -> io::Result<PathBuf> {
    if let Some(socket_path) = env::var_os(hearthspace_common::SETTINGS_SOCKET_ENV) {
        return Ok(PathBuf::from(socket_path));
    }

    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set"))?;
    Ok(PathBuf::from(runtime_dir).join("hearthspace-settingsd.sock"))
}

pub fn client_get_settings() -> io::Result<Settings> {
    send_settings_request("get\n")
}

pub fn client_set_test(test: bool) -> io::Result<Settings> {
    let value = if test { "true" } else { "false" };
    send_settings_request(&format!("set test {value}\n"))
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

pub fn parse_settings_request(line: &str) -> Result<SettingsRequest, String> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["get"] => Ok(SettingsRequest::Get),
        ["set", "test", value] => Ok(SettingsRequest::SetTest(parse_bool(value)?)),
        ["set", "monitor", id, x, y] => Ok(SettingsRequest::SetMonitorPosition {
            id: (*id).to_string(),
            x: parse_i32(x, "monitor x")?,
            y: parse_i32(y, "monitor y")?,
        }),
        ["set", "monitor-scale", id, scale] => Ok(SettingsRequest::SetMonitorScale {
            id: (*id).to_string(),
            scale: parse_positive_i32(scale, "monitor scale")?,
        }),
        [] => Err("empty request".into()),
        _ => Err(format!("unknown request: {line:?}")),
    }
}

pub fn settings_response(settings: Settings) -> String {
    let mut response = format!("ok test {}", settings.test);
    for monitor in settings.monitors {
        response.push_str(&format!(
            " monitor {} {} {} {} {} {} {}",
            encode_token(&monitor.id),
            monitor.width,
            monitor.height,
            monitor.scale,
            monitor.x,
            monitor.y,
            encode_token(&monitor.name)
        ));
    }
    response.push('\n');
    response
}

pub fn error_response(error: impl std::fmt::Display) -> String {
    format!("err {error}\n")
}

fn send_settings_request(request: &str) -> io::Result<Settings> {
    let mut stream = UnixStream::connect(settings_socket_path()?)?;
    stream.write_all(request.as_bytes())?;
    stream.shutdown(Shutdown::Write)?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    parse_settings_response(&response)
}

fn parse_settings_response(response: &str) -> io::Result<Settings> {
    let line = response.trim();
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["ok", "test", value, rest @ ..] => parse_settings_response_parts(value, rest),
        ["err", message @ ..] => Err(io::Error::other(message.join(" "))),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid settingsd response: {line:?}"),
        )),
    }
}

fn parse_settings_response_parts(test: &str, rest: &[&str]) -> io::Result<Settings> {
    let mut settings = Settings {
        test: parse_bool(test).map_err(io::Error::other)?,
        monitors: Vec::new(),
    };
    let chunks = rest.chunks_exact(8);
    if !chunks.remainder().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid settingsd response monitor field count",
        ));
    }
    for monitor in chunks {
        if monitor[0] != "monitor" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid settingsd response monitor marker: {:?}",
                    monitor[0]
                ),
            ));
        }
        settings.monitors.push(MonitorConfig {
            id: decode_token(monitor[1])?,
            width: parse_i32(monitor[2], "monitor width").map_err(io::Error::other)?,
            height: parse_i32(monitor[3], "monitor height").map_err(io::Error::other)?,
            scale: parse_positive_i32(monitor[4], "monitor scale").map_err(io::Error::other)?,
            x: parse_i32(monitor[5], "monitor x").map_err(io::Error::other)?,
            y: parse_i32(monitor[6], "monitor y").map_err(io::Error::other)?,
            name: decode_token(monitor[7])?,
        });
    }
    if settings.monitors.is_empty() {
        settings.monitors = Settings::default().monitors;
    }
    Ok(settings)
}

fn encode_token(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn decode_token(value: &str) -> io::Result<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid encoded token: {value:?}"),
                ));
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid token: {error}"),
                )
            })?;
            decoded.push(u8::from_str_radix(hex, 16).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid token: {error}"),
                )
            })?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn parse_i32(value: &str, label: &str) -> Result<i32, String> {
    value
        .parse::<i32>()
        .map_err(|error| format!("invalid {label}: {error}"))
}

fn parse_positive_i32(value: &str, label: &str) -> Result<i32, String> {
    let value = parse_i32(value, label)?;
    if value <= 0 {
        return Err(format!("{label} must be positive"));
    }
    Ok(value)
}

fn validate_monitor_id(id: &str) -> io::Result<()> {
    if id.is_empty() || id.chars().any(char::is_whitespace) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("monitor id cannot contain whitespace: {id:?}"),
        ));
    }
    Ok(())
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("expected true or false, got {value:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ipc_requests() {
        assert_eq!(parse_settings_request("get\n"), Ok(SettingsRequest::Get));
        assert_eq!(
            parse_settings_request("set test true\n"),
            Ok(SettingsRequest::SetTest(true))
        );
        assert_eq!(
            parse_settings_request("set monitor primary 120 -40\n"),
            Ok(SettingsRequest::SetMonitorPosition {
                id: "primary".to_string(),
                x: 120,
                y: -40,
            })
        );
        assert_eq!(
            parse_settings_request("set monitor-scale primary 2\n"),
            Ok(SettingsRequest::SetMonitorScale {
                id: "primary".to_string(),
                scale: 2,
            })
        );
    }

    #[test]
    fn parses_ipc_response() {
        assert_eq!(
            parse_settings_response(
                "ok test true monitor primary 1920 1080 2 120 -40 Primary monitor secondary 1280 720 1 2040 0 Secondary%20Display\n"
            )
            .unwrap(),
            Settings {
                test: true,
                monitors: vec![
                    MonitorConfig {
                        id: "primary".to_string(),
                        name: "Primary".to_string(),
                        width: 1920,
                        height: 1080,
                        scale: 2,
                        x: 120,
                        y: -40,
                    },
                    MonitorConfig {
                        id: "secondary".to_string(),
                        name: "Secondary Display".to_string(),
                        width: 1280,
                        height: 720,
                        scale: 1,
                        x: 2040,
                        y: 0,
                    },
                ],
            }
        );
    }
}
