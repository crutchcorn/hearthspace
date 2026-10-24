use crate::{display_layout, *};
use std::io;

pub fn parse_settings_request(line: &str) -> Result<SettingsRequest, String> {
    if let Some(payload) = line.trim().strip_prefix("set display-layout ") {
        return display_layout::parse_request(payload).map(SettingsRequest::SetDisplayLayout);
    }
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["get"] => Ok(SettingsRequest::Get),
        ["set", "test", value] => Ok(SettingsRequest::SetTest(parse_bool(value)?)),
        ["set", "color-mode", value] => Ok(SettingsRequest::SetColorMode(value.parse()?)),
        ["set", "accent-color", value] => Ok(SettingsRequest::SetAccentColor(value.parse()?)),
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
            .map(encode_background_path)
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
    response.push_str(&format!(
        " color-mode {} accent-color {}\n",
        settings.appearance.color_mode, settings.appearance.accent_color
    ));
    response
}

pub fn error_response(error: impl std::fmt::Display) -> String {
    format!("err {error}\n")
}

pub(crate) fn parse_settings_response(response: &str) -> io::Result<Settings> {
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
    mut rest: &[&str],
) -> io::Result<Settings> {
    let mut settings = Settings {
        test: parse_bool(test).map_err(io::Error::other)?,
        background_mode,
        background_image_path,
        monitors: Vec::new(),
        appearance: Appearance::default(),
    };
    while !rest.is_empty() {
        rest = match rest {
            ["color-mode", value, rest @ ..] => {
                settings.appearance.color_mode = value.parse().map_err(io::Error::other)?;
                rest
            }
            ["accent-color", value, rest @ ..] => {
                settings.appearance.accent_color = value.parse().map_err(io::Error::other)?;
                rest
            }
            ["monitor", id, width, height, scale, x, y, name, rest @ ..] => {
                settings.monitors.push(MonitorConfig {
                    id: decode_token(id)?,
                    width: parse_i32(width, "monitor width").map_err(io::Error::other)?,
                    height: parse_i32(height, "monitor height").map_err(io::Error::other)?,
                    scale: parse_positive_i32(scale, "monitor scale").map_err(io::Error::other)?,
                    x: parse_i32(x, "monitor x").map_err(io::Error::other)?,
                    y: parse_i32(y, "monitor y").map_err(io::Error::other)?,
                    name: decode_token(name)?,
                });
                rest
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid settingsd response fields",
                ));
            }
        };
    }
    if settings.monitors.is_empty() {
        settings.monitors = Settings::default().monitors;
    }
    Ok(settings)
}

fn encode_token(value: &str) -> String {
    if value.is_empty() {
        return "~".into();
    }
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
    if value == "~" {
        return Ok(String::new());
    }
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

pub(crate) fn encode_background_path(value: &str) -> String {
    // The old wire sentinel must not swallow a literal file named "default".
    if value == "default" {
        "%64efault".into()
    } else {
        encode_token(value)
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

pub(crate) fn validate_monitor_id(id: &str) -> io::Result<()> {
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
