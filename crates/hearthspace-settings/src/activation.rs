//! A Settings instance belongs to one Hearthspace session. Later launches send
//! only a bounded page name to that instance; the compositor owns window focus.
use std::{
    fs, io,
    io::{Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    time::Duration,
};
use xilem::{
    ViewCtx,
    core::{MessageProxy, NoElement, View},
    tokio::{io::AsyncWriteExt, time::timeout},
    view::task_raw,
};

use crate::{
    cli::parse_page,
    state::{SettingsApp, SettingsCategory},
};

const LIMIT: usize = 64;
const DEADLINE: Duration = Duration::from_secs(3);

pub(crate) enum Startup {
    Primary(Option<Endpoint>),
    Forwarded,
}

pub(crate) struct Endpoint {
    pub(crate) listener: UnixListener,
    path: PathBuf,
    identity: (u64, u64),
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        // Never remove a replacement endpoint, symlink, or unrelated file.
        if socket_identity(&self.path).ok() == Some(self.identity) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub(crate) fn start(category: SettingsCategory) -> io::Result<Startup> {
    let Some(command) = std::env::var_os(hearthspace_common::SHELL_COMMAND_SOCKET_ENV) else {
        // Standalone Settings does not claim any host-wide singleton.
        return Ok(Startup::Primary(None));
    };
    let command = PathBuf::from(command);
    if !command.is_absolute() || command.parent().is_none() {
        return Err(io::Error::other(
            "Settings requires an absolute session command socket",
        ));
    }
    let mut endpoint = command.into_os_string();
    endpoint.push(".settings");
    start_at(&PathBuf::from(endpoint), category)
}

fn socket_identity(path: &Path) -> io::Result<(u64, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_socket() {
        return Err(io::Error::other("Settings activation path is not a socket"));
    }
    Ok((metadata.dev(), metadata.ino()))
}

fn start_at(path: &Path, category: SettingsCategory) -> io::Result<Startup> {
    // Hold one stable lock inode through stale detection and replacement.
    // Removing the lock file would allow simultaneous elections on two inodes.
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(".lock");
    let election_lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock_path)?;
    election_lock.lock()?;
    match UnixListener::bind(path) {
        Ok(listener) => {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            listener.set_nonblocking(true)?;
            Ok(Startup::Primary(Some(Endpoint {
                listener,
                path: path.into(),
                identity: socket_identity(path)?,
            })))
        }
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
            let identity = socket_identity(path)?;
            match UnixStream::connect(path) {
                Ok(mut stream) => {
                    // The primary task can now accept activation while another
                    // launcher independently completes its own election.
                    drop(election_lock);
                    stream.set_read_timeout(Some(DEADLINE))?;
                    stream.set_write_timeout(Some(DEADLINE))?;
                    writeln!(stream, "{}", category.label().to_lowercase())?;
                    let mut response = [0; 3];
                    stream.read_exact(&mut response)?;
                    if response != *b"ok\n" {
                        return Err(io::Error::other("Settings rejected the activation request"));
                    }
                    Ok(Startup::Forwarded)
                }
                Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    if socket_identity(path)? != identity {
                        return Err(io::Error::other("Settings activation endpoint changed"));
                    }
                    fs::remove_file(path)?;
                    // A concurrent primary may win this bind; never remove it.
                    let listener = UnixListener::bind(path)?;
                    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
                    listener.set_nonblocking(true)?;
                    Ok(Startup::Primary(Some(Endpoint {
                        listener,
                        path: path.into(),
                        identity: socket_identity(path)?,
                    })))
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

async fn read_page(stream: &mut xilem::tokio::net::UnixStream) -> io::Result<SettingsCategory> {
    use xilem::tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    loop {
        let byte = stream.read_u8().await?;
        if byte == b'\n' {
            return std::str::from_utf8(&bytes)
                .ok()
                .and_then(|page| parse_page(page).ok())
                .ok_or_else(|| io::Error::other("Unknown Settings page"));
        }
        bytes.push(byte);
        if bytes.len() >= LIMIT {
            return Err(io::Error::other("Settings activation request is too long"));
        }
    }
}

pub(crate) fn activate(state: &mut SettingsApp, category: SettingsCategory) {
    // Reopening the same editor must not discard an unsaved gesture.
    if state.selected_category != category {
        state.select_category(category);
    }
}

pub(crate) fn task() -> impl View<SettingsApp, (), ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<SettingsCategory>, state: &mut SettingsApp| {
            let listener = state.activation_listener.take();
            async move {
                let Some(listener) = listener else { return };
                let Ok(listener) = xilem::tokio::net::UnixListener::from_std(listener) else {
                    return;
                };
                while let Ok((mut stream, _)) = listener.accept().await {
                    if let Ok(Ok(category)) = timeout(DEADLINE, read_page(&mut stream)).await {
                        if proxy.message(category).is_err() {
                            break;
                        }
                        let _ = timeout(DEADLINE, stream.write_all(b"ok\n")).await;
                    }
                }
            }
        },
        activate,
    )
}

#[cfg(test)]
mod tests;
