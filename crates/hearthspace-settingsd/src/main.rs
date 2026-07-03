use std::{
    fs, io,
    io::{BufRead, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    process,
};

use hearthspace_ipc::{
    MonitorConfig, Settings, SettingsRequest, error_response, parse_settings_request,
    settings_response, settings_socket_path,
};
use tracing::{error, info, warn};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    initialize_tracing();

    let listener = match bind_settings_socket()? {
        BindSettingsSocket::Listening(listener) => listener,
        BindSettingsSocket::AlreadyRunning(path) => {
            warn!(socket = %path.display(), "hearthspace settings daemon is already running");
            return Ok(());
        }
    };
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

enum BindSettingsSocket {
    Listening(UnixListener),
    AlreadyRunning(PathBuf),
}

fn bind_settings_socket() -> io::Result<BindSettingsSocket> {
    let path = settings_socket_path()?;
    bind_settings_socket_at(path)
}

fn bind_settings_socket_at(path: PathBuf) -> io::Result<BindSettingsSocket> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    if UnixStream::connect(&path).is_ok() {
        return Ok(BindSettingsSocket::AlreadyRunning(path));
    }

    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    UnixListener::bind(path).map(BindSettingsSocket::Listening)
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
            save_settings_to_path(settings.clone(), config_path)?;
            Ok(settings_response(settings))
        }
        SettingsRequest::SetMonitorPosition { id, x, y } => {
            let mut settings = load_settings_from_path(config_path)?;
            let Some(monitor) = settings
                .monitors
                .iter_mut()
                .find(|monitor| monitor.id == id)
            else {
                return Ok(error_response(format!("unknown monitor id: {id}")));
            };
            monitor.x = x;
            monitor.y = y;
            save_settings_to_path(settings.clone(), config_path)?;
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
    let temp_path = path.with_extension(format!("tmp-{}", process::id()));
    let mut file = fs::File::create(&temp_path)?;
    file.write_all(format_settings(settings).as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp_path, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp_path);
    })
}

fn parse_settings(contents: &str) -> io::Result<Settings> {
    let mut settings = Settings::default();
    settings.monitors.clear();
    let mut current_monitor: Option<MonitorConfig> = None;
    for raw_line in contents.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }

        if line == "[[monitor]]" {
            if let Some(monitor) = current_monitor.take() {
                settings.monitors.push(monitor);
            }
            current_monitor = Some(MonitorConfig {
                id: String::new(),
                name: String::new(),
                width: 1920,
                height: 1080,
                x: 0,
                y: 0,
            });
            continue;
        }

        if line.starts_with('[') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid settings line: {line:?}"),
            ));
        };
        let key = key.trim();
        let value = value.trim();

        if let Some(monitor) = &mut current_monitor {
            match key {
                "id" => monitor.id = value.to_string(),
                "name" => monitor.name = value.to_string(),
                "width" => monitor.width = parse_i32(value, "monitor width")?,
                "height" => monitor.height = parse_i32(value, "monitor height")?,
                "x" => monitor.x = parse_i32(value, "monitor x")?,
                "y" => monitor.y = parse_i32(value, "monitor y")?,
                _ => {}
            }
        } else if key == "test" {
            settings.test = parse_bool(value).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid test setting: {error}"),
                )
            })?;
        }
    }

    if let Some(monitor) = current_monitor.take() {
        settings.monitors.push(monitor);
    }
    settings.monitors.retain(|monitor| !monitor.id.is_empty());
    if settings.monitors.is_empty() {
        settings.monitors = Settings::default().monitors;
    }
    Ok(settings)
}

fn format_settings(settings: Settings) -> String {
    let mut contents = format!("test = {}\n", settings.test);
    for monitor in settings.monitors {
        contents.push_str(&format!(
            "\n[[monitor]]\n\
             id = {}\n\
             name = {}\n\
             width = {}\n\
             height = {}\n\
             x = {}\n\
             y = {}\n",
            monitor.id, monitor.name, monitor.width, monitor.height, monitor.x, monitor.y,
        ));
    }
    contents
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("expected true or false, got {value:?}")),
    }
}

fn parse_i32(value: &str, label: &str) -> io::Result<i32> {
    value.parse::<i32>().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid {label}: {error}"),
        )
    })
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

    use super::{BindSettingsSocket, bind_settings_socket_at, handle_connection_with_config_path};

    #[test]
    fn get_returns_default_settings_when_config_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("hearthspace/config.toml");

        let response = round_trip("get\n", &config_path);

        assert!(response.starts_with("ok test false monitor primary 1920 1080 0 0 Primary"));
        assert!(response.contains("monitor secondary 1920 1080 1920 0 Secondary"));
        assert!(!config_path.exists());
    }

    #[test]
    fn get_reads_existing_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("hearthspace/config.toml");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            "test = true\n\
             \n\
             [[monitor]]\n\
             id = dock\n\
             name = Dock Display\n\
             width = 2560\n\
             height = 1440\n\
             x = -2560\n\
             y = 120\n",
        )
        .unwrap();

        let response = round_trip("get\n", &config_path);

        assert_eq!(
            response,
            "ok test true monitor dock 2560 1440 -2560 120 Dock%20Display\n"
        );
    }

    #[test]
    fn set_test_writes_config_and_returns_saved_settings() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("hearthspace/config.toml");

        let response = round_trip("set test true\n", &config_path);

        assert!(response.starts_with("ok test true monitor primary 1920 1080 0 0 Primary"));
        let contents = fs::read_to_string(config_path).unwrap();
        assert!(contents.starts_with("test = true\n"));
        assert!(contents.contains("[[monitor]]\nid = primary\n"));
    }

    #[test]
    fn set_monitor_position_writes_config_and_returns_layout() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("hearthspace/config.toml");

        let response = round_trip("set monitor secondary -1600 90\n", &config_path);

        assert!(response.contains("monitor secondary 1920 1080 -1600 90 Secondary"));
        let contents = fs::read_to_string(config_path).unwrap();
        assert!(contents.contains("id = secondary\n"));
        assert!(contents.contains("x = -1600\n"));
        assert!(contents.contains("y = 90\n"));
    }

    #[test]
    fn set_monitor_position_rejects_unknown_monitor() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("hearthspace/config.toml");

        let response = round_trip("set monitor missing 10 20\n", &config_path);

        assert_eq!(response, "err unknown monitor id: missing\n");
        assert!(!config_path.exists());
    }

    #[test]
    fn invalid_request_returns_error_without_writing_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("hearthspace/config.toml");

        let response = round_trip("set nope true\n", &config_path);

        assert!(response.starts_with("err unknown request:"));
        assert!(!config_path.exists());
    }

    #[test]
    fn bind_exits_cleanly_when_listener_is_already_active() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("settings.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

        let result = bind_settings_socket_at(socket_path.clone()).unwrap();

        assert!(matches!(result, BindSettingsSocket::AlreadyRunning(path) if path == socket_path));
    }

    #[test]
    fn bind_removes_stale_socket_file() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("settings.sock");
        std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

        let result = bind_settings_socket_at(socket_path).unwrap();

        assert!(matches!(result, BindSettingsSocket::Listening(_)));
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
