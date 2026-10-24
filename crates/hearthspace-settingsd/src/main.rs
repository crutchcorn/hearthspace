use std::{
    fs, io,
    os::unix::net::UnixListener,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[cfg(test)]
use hearthspace_ipc::error_response;
use hearthspace_ipc::{SettingsRequest, settings_response, settings_socket_path};
use tracing::{info, warn};

mod mutation;
mod server;
mod storage;

use storage::load_settings_from_path;

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

    server::serve(listener, &hearthspace_common::config_path()?)?;
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

    match hearthspace_ipc::transport::connect_until(
        &path,
        Instant::now() + Duration::from_millis(50),
    ) {
        Ok(_) => return Ok(BindSettingsSocket::AlreadyRunning(path)),
        // A live listener with a full backlog is not a stale socket.
        Err(error) if error.kind() == io::ErrorKind::TimedOut => {
            return Ok(BindSettingsSocket::AlreadyRunning(path));
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            ) => {}
        Err(error) => return Err(error),
    }

    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    UnixListener::bind(path).map(BindSettingsSocket::Listening)
}

#[cfg(test)]
fn handle_connection_with_config_path(
    mut stream: std::os::unix::net::UnixStream,
    config_path: &Path,
) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_millis(500);
    let request = hearthspace_ipc::transport::read_line_until(
        &mut stream,
        hearthspace_ipc::transport::MAX_REQUEST_BYTES,
        deadline,
    )?;
    let response = match hearthspace_ipc::parse_settings_request(&request) {
        Ok(request) => handle_request(request, config_path).unwrap_or_else(error_response),
        Err(error) => error_response(error),
    };

    hearthspace_ipc::transport::write_until(&mut stream, response.as_bytes(), deadline)
}

fn handle_request(request: SettingsRequest, config_path: &Path) -> io::Result<String> {
    if request == SettingsRequest::Get {
        return Ok(settings_response(load_settings_from_path(config_path)?));
    }
    let snapshot = storage::update(config_path, |settings| mutation::apply(settings, request))?;
    Ok(settings_response(snapshot.settings))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod transactions;
