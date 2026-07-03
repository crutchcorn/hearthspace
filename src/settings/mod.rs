use std::{
    env, fs, io,
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
};

const CONFIG_DIR_NAME: &str = "hearthspace";
const CONFIG_FILE_NAME: &str = "config.toml";
const SETTINGS_SOCKET_NAME: &str = "hearthspace-settingsd.sock";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Settings {
    pub test: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRequest {
    Get,
    SetTest(bool),
}

pub fn config_path() -> io::Result<PathBuf> {
    Ok(config_home()?.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME))
}

pub fn settings_socket_path() -> io::Result<PathBuf> {
    if let Some(socket_path) = env::var_os(hearthspace_common::SETTINGS_SOCKET_ENV) {
        return Ok(PathBuf::from(socket_path));
    }

    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set"))?;
    Ok(PathBuf::from(runtime_dir).join(SETTINGS_SOCKET_NAME))
}

pub fn load_settings() -> io::Result<Settings> {
    load_settings_from_path(&config_path()?)
}

pub fn load_settings_from_path(path: impl AsRef<Path>) -> io::Result<Settings> {
    match fs::read_to_string(path) {
        Ok(contents) => parse_settings(&contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(error) => Err(error),
    }
}

pub fn save_settings(settings: Settings) -> io::Result<()> {
    save_settings_to_path(settings, config_path()?)
}

pub fn save_settings_to_path(settings: Settings, path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format_settings(settings))
}

pub fn client_get_settings() -> io::Result<Settings> {
    send_settings_request("get\n")
}

pub fn client_set_test(test: bool) -> io::Result<Settings> {
    let value = if test { "true" } else { "false" };
    send_settings_request(&format!("set test {value}\n"))
}

pub fn parse_settings_request(line: &str) -> Result<SettingsRequest, String> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["get"] => Ok(SettingsRequest::Get),
        ["set", "test", value] => Ok(SettingsRequest::SetTest(parse_bool(value)?)),
        [] => Err("empty request".into()),
        _ => Err(format!("unknown request: {line:?}")),
    }
}

pub fn settings_response(settings: Settings) -> String {
    format!("ok test {}\n", settings.test)
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

fn config_home() -> io::Result<PathBuf> {
    if let Some(xdg_config_home) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg_config_home));
    }

    let home = env::var_os("HOME").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "neither XDG_CONFIG_HOME nor HOME is set",
        )
    })?;
    Ok(PathBuf::from(home).join(".config"))
}

fn parse_settings(contents: &str) -> io::Result<Settings> {
    let mut settings = Settings::default();
    for raw_line in contents.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() || line.starts_with('[') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid settings line: {line:?}"),
            ));
        };

        if key.trim() == "test" {
            settings.test = parse_bool(value.trim()).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid test setting: {error}"),
                )
            })?;
        }
    }
    Ok(settings)
}

fn format_settings(settings: Settings) -> String {
    format!("test = {}\n", settings.test)
}

fn parse_settings_response(response: &str) -> io::Result<Settings> {
    let line = response.trim();
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["ok", "test", value] => Ok(Settings {
            test: parse_bool(value).map_err(io::Error::other)?,
        }),
        ["err", message @ ..] => Err(io::Error::other(message.join(" "))),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid settingsd response: {line:?}"),
        )),
    }
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
    fn parses_test_setting() {
        assert_eq!(
            parse_settings("test = true\n").unwrap(),
            Settings { test: true }
        );
        assert_eq!(
            parse_settings("test = false\n").unwrap(),
            Settings { test: false }
        );
    }

    #[test]
    fn formats_test_setting() {
        assert_eq!(format_settings(Settings { test: true }), "test = true\n");
    }

    #[test]
    fn parses_ipc_requests() {
        assert_eq!(parse_settings_request("get\n"), Ok(SettingsRequest::Get));
        assert_eq!(
            parse_settings_request("set test true\n"),
            Ok(SettingsRequest::SetTest(true))
        );
    }

    #[test]
    fn parses_ipc_response() {
        assert_eq!(
            parse_settings_response("ok test true\n").unwrap(),
            Settings { test: true }
        );
    }
}
