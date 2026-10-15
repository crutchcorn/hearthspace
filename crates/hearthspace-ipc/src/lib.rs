use std::{
    env, io,
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
    str::FromStr,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub test: bool,
    pub background_mode: BackgroundMode,
    pub background_image_path: Option<String>,
    pub monitors: Vec<MonitorConfig>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BackgroundMode {
    #[default]
    RepeatWithCanvas,
    Stretch,
    Contains,
    Cover,
}

impl BackgroundMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RepeatWithCanvas => "repeat-with-canvas",
            Self::Stretch => "stretch",
            Self::Contains => "contains",
            Self::Cover => "cover",
        }
    }

    pub fn from_legacy_moves_with_canvas(value: bool) -> Self {
        if value {
            Self::RepeatWithCanvas
        } else {
            Self::Cover
        }
    }
}

impl std::fmt::Display for BackgroundMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for BackgroundMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "repeat-with-canvas" => Ok(Self::RepeatWithCanvas),
            "stretch" => Ok(Self::Stretch),
            "contains" => Ok(Self::Contains),
            "cover" => Ok(Self::Cover),
            _ => Err(format!("invalid background mode: {value:?}")),
        }
    }
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
            background_mode: BackgroundMode::default(),
            background_image_path: None,
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
    SetBackgroundMode(BackgroundMode),
    SetBackgroundImagePath(Option<String>),
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

pub fn client_set_background_mode(mode: BackgroundMode) -> io::Result<Settings> {
    send_settings_request(&format!("set background-mode {mode}\n"))
}

pub fn client_set_background_image_path(path: Option<&str>) -> io::Result<Settings> {
    let path = path
        .map(encode_token)
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

pub fn parse_settings_request(line: &str) -> Result<SettingsRequest, String> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["get"] => Ok(SettingsRequest::Get),
        ["set", "test", value] => Ok(SettingsRequest::SetTest(parse_bool(value)?)),
        ["set", "background-mode", value] => Ok(SettingsRequest::SetBackgroundMode(
            value.parse::<BackgroundMode>()?,
        )),
        ["set", "background-moves-with-canvas", value] => Ok(SettingsRequest::SetBackgroundMode(
            BackgroundMode::from_legacy_moves_with_canvas(parse_bool(value)?),
        )),
        ["set", "background-image-path", value] => Ok(SettingsRequest::SetBackgroundImagePath(
            parse_background_image_path(value).map_err(|error| error.to_string())?,
        )),
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
    let mut response = format!(
        "ok test {} background-mode {} background-image-path {}",
        settings.test,
        settings.background_mode,
        settings
            .background_image_path
            .as_deref()
            .map(encode_token)
            .unwrap_or_else(|| "default".to_string())
    );
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
        [
            "ok",
            "test",
            test,
            "background-mode",
            background_mode,
            "background-image-path",
            background_image_path,
            rest @ ..,
        ] => parse_settings_response_parts(
            test,
            background_mode
                .parse::<BackgroundMode>()
                .map_err(io::Error::other)?,
            parse_background_image_path(background_image_path)?,
            rest,
        ),
        [
            "ok",
            "test",
            test,
            "background-mode",
            background_mode,
            rest @ ..,
        ] => parse_settings_response_parts(
            test,
            background_mode
                .parse::<BackgroundMode>()
                .map_err(io::Error::other)?,
            None,
            rest,
        ),
        [
            "ok",
            "test",
            test,
            "background-moves-with-canvas",
            background_moves_with_canvas,
            "background-image-path",
            background_image_path,
            rest @ ..,
        ] => parse_settings_response_parts(
            test,
            BackgroundMode::from_legacy_moves_with_canvas(
                parse_bool(background_moves_with_canvas).map_err(io::Error::other)?,
            ),
            parse_background_image_path(background_image_path)?,
            rest,
        ),
        [
            "ok",
            "test",
            test,
            "background-moves-with-canvas",
            background_moves_with_canvas,
            rest @ ..,
        ] => parse_settings_response_parts(
            test,
            BackgroundMode::from_legacy_moves_with_canvas(
                parse_bool(background_moves_with_canvas).map_err(io::Error::other)?,
            ),
            None,
            rest,
        ),
        ["ok", "test", test, rest @ ..] => {
            parse_settings_response_parts(test, Settings::default().background_mode, None, rest)
        }
        ["err", message @ ..] => Err(io::Error::other(message.join(" "))),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid settingsd response: {line:?}"),
        )),
    }
}

fn parse_settings_response_parts(
    test: &str,
    background_mode: BackgroundMode,
    background_image_path: Option<String>,
    rest: &[&str],
) -> io::Result<Settings> {
    let mut settings = Settings {
        test: parse_bool(test).map_err(io::Error::other)?,
        background_mode,
        background_image_path,
        monitors: Vec::new(),
    };
    let (chunks, remainder) = rest.as_chunks::<8>();
    if !remainder.is_empty() {
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

fn parse_background_image_path(value: &str) -> io::Result<Option<String>> {
    if value == "default" {
        Ok(None)
    } else {
        decode_token(value).map(Some)
    }
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
            parse_settings_request("set background-mode cover\n"),
            Ok(SettingsRequest::SetBackgroundMode(BackgroundMode::Cover))
        );
        assert_eq!(
            parse_settings_request("set background-moves-with-canvas false\n"),
            Ok(SettingsRequest::SetBackgroundMode(BackgroundMode::Cover))
        );
        assert_eq!(
            parse_settings_request("set background-image-path %2Ftmp%2Fwallpaper.png\n"),
            Ok(SettingsRequest::SetBackgroundImagePath(Some(
                "/tmp/wallpaper.png".to_string()
            )))
        );
        assert_eq!(
            parse_settings_request("set background-image-path default\n"),
            Ok(SettingsRequest::SetBackgroundImagePath(None))
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
                "ok test true background-mode contains background-image-path %2Ftmp%2Fwallpaper.png monitor primary 1920 1080 2 120 -40 Primary monitor secondary 1280 720 1 2040 0 Secondary%20Display\n"
            )
            .unwrap(),
            Settings {
                test: true,
                background_mode: BackgroundMode::Contains,
                background_image_path: Some("/tmp/wallpaper.png".to_string()),
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

    #[test]
    fn parses_legacy_ipc_response_with_background_moves_with_canvas() {
        assert_eq!(
            parse_settings_response(
                "ok test true background-moves-with-canvas false background-image-path %2Ftmp%2Fwallpaper.png monitor primary 1920 1080 2 120 -40 Primary\n"
            )
            .unwrap()
            .background_mode,
            BackgroundMode::Cover
        );
    }

    #[test]
    fn parses_legacy_ipc_response_with_default_background_setting() {
        assert_eq!(
            parse_settings_response("ok test true monitor primary 1920 1080 1 0 0 Primary\n")
                .unwrap()
                .background_mode,
            Settings::default().background_mode
        );
    }
}
