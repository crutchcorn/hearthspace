use std::{
    env, io,
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Settings {
    pub test: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRequest {
    Get,
    SetTest(bool),
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
