use std::{
    fs, io,
    io::{BufRead, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
};

use hearthspace_ipc::{
    Settings, SettingsRequest, error_response, parse_settings_request, settings_response,
    settings_socket_path,
};
use tracing::{error, info, warn};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    initialize_tracing();

    let listener = bind_settings_socket()?;
    info!("hearthspace settings daemon is listening");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle_connection(stream) {
                    warn!(%error, "failed to handle settings client");
                }
            }
            Err(error) => error!(%error, "failed to accept settings client"),
        }
    }

    Ok(())
}

fn initialize_tracing() {
    match tracing_subscriber::EnvFilter::try_from_default_env() {
        Ok(env_filter) => tracing_subscriber::fmt().with_env_filter(env_filter).init(),
        Err(_) => tracing_subscriber::fmt().init(),
    }
}

fn bind_settings_socket() -> io::Result<UnixListener> {
    let path = settings_socket_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    if UnixStream::connect(&path).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            format!("settings daemon is already listening at {}", path.display()),
        ));
    }

    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    UnixListener::bind(path)
}

fn handle_connection(stream: UnixStream) -> io::Result<()> {
    handle_connection_with_config_path(stream, &hearthspace_common::config_path()?)
}

fn handle_connection_with_config_path(
    mut stream: UnixStream,
    config_path: &Path,
) -> io::Result<()> {
    let mut request = String::new();
    {
        let mut reader = io::BufReader::new(&stream);
        reader.read_line(&mut request)?;
    }

    let response = match parse_settings_request(&request) {
        Ok(request) => handle_request(request, config_path).unwrap_or_else(error_response),
        Err(error) => error_response(error),
    };

    stream.write_all(response.as_bytes())
}

fn handle_request(request: SettingsRequest, config_path: &Path) -> io::Result<String> {
    match request {
        SettingsRequest::Get => Ok(settings_response(load_settings_from_path(config_path)?)),
        SettingsRequest::SetTest(test) => {
            let mut settings = load_settings_from_path(config_path)?;
            settings.test = test;
            save_settings_to_path(settings, config_path)?;
            Ok(settings_response(settings))
        }
    }
}

fn load_settings_from_path(path: impl AsRef<Path>) -> io::Result<Settings> {
    match fs::read_to_string(path) {
        Ok(contents) => parse_settings(&contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(error) => Err(error),
    }
}

fn save_settings_to_path(settings: Settings, path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format_settings(settings))
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

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("expected true or false, got {value:?}")),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        net::Shutdown,
        os::unix::net::UnixStream,
        path::Path,
    };

    use super::handle_connection_with_config_path;

    #[test]
    fn get_returns_default_settings_when_config_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("hearthspace/config.toml");

        let response = round_trip("get\n", &config_path);

        assert_eq!(response, "ok test false\n");
        assert!(!config_path.exists());
    }

    #[test]
    fn get_reads_existing_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("hearthspace/config.toml");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(&config_path, "test = true\n").unwrap();

        let response = round_trip("get\n", &config_path);

        assert_eq!(response, "ok test true\n");
    }

    #[test]
    fn set_test_writes_config_and_returns_saved_settings() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("hearthspace/config.toml");

        let response = round_trip("set test true\n", &config_path);

        assert_eq!(response, "ok test true\n");
        assert_eq!(fs::read_to_string(config_path).unwrap(), "test = true\n");
    }

    #[test]
    fn invalid_request_returns_error_without_writing_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("hearthspace/config.toml");

        let response = round_trip("set nope true\n", &config_path);

        assert!(response.starts_with("err unknown request:"));
        assert!(!config_path.exists());
    }

    fn round_trip(request: &str, config_path: &Path) -> String {
        let (server, mut client) = UnixStream::pair().unwrap();
        client.write_all(request.as_bytes()).unwrap();
        client.shutdown(Shutdown::Write).unwrap();

        handle_connection_with_config_path(server, config_path).unwrap();

        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        response
    }
}
