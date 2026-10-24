//! Cancellable observation on a dedicated Wayland connection.
use crate::{OutputInfo, state::OutputWatcher};
use std::{
    io,
    os::fd::{AsFd, BorrowedFd},
    path::PathBuf,
};
use tokio::io::{Interest, Ready, unix::AsyncFd};
use wayland_client::{Connection, EventQueue, backend::WaylandError};

pub async fn connect_wayland() -> io::Result<Connection> {
    if std::env::var("WAYLAND_SOCKET").is_ok() {
        // This adopts an already-connected fd; it does not wait for a server.
        return Connection::connect_to_env().map_err(io::Error::other);
    }
    let no_compositor = || io::Error::other("could not find Wayland compositor");
    let mut path = PathBuf::from(std::env::var_os("WAYLAND_DISPLAY").ok_or_else(no_compositor)?);
    if !path.is_absolute() {
        let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(no_compositor)?);
        if !runtime.is_absolute() {
            return Err(no_compositor());
        }
        path = runtime.join(path);
    }
    // A full listener backlog must not block the runtime during bootstrap.
    let socket = tokio::net::UnixStream::connect(path).await?.into_std()?;
    Connection::from_socket(socket).map_err(io::Error::other)
}

pub async fn watch_outputs(
    connection: Connection,
    mut send_outputs: impl FnMut(Vec<OutputInfo>) -> io::Result<()>,
) -> io::Result<()> {
    let mut event_queue = connection.new_event_queue();
    let queue_handle = event_queue.handle();
    let _registry = connection.display().get_registry(&queue_handle, ());
    let mut watcher = OutputWatcher::default();
    // SAFETY: connection keeps this descriptor open and stable. This local
    // registration is never forgotten and is dropped before connection.
    let socket = unsafe { AsyncFd::register(connection.as_fd()) }?;
    // Keep both startup barriers: registry globals first, then bound outputs'
    // initial names. These use the same cancellable I/O as the hotplug loop.
    for _ in 0..2 {
        watcher.sync_done = false;
        connection.display().sync(&queue_handle, ());
        while !watcher.sync_done {
            dispatch_outputs(&mut event_queue, &socket, &mut watcher).await?;
        }
    }
    watcher.take_changed();
    send_outputs(watcher.snapshot())?;

    loop {
        dispatch_outputs(&mut event_queue, &socket, &mut watcher).await?;
        if watcher.take_changed() {
            send_outputs(watcher.snapshot())?;
        }
    }
}

async fn dispatch_outputs(
    queue: &mut EventQueue<OutputWatcher>,
    socket: &AsyncFd<BorrowedFd<'_>>,
    watcher: &mut OutputWatcher,
) -> io::Result<()> {
    loop {
        // AsyncFd::ready does not consume Tokio's cooperative budget. Always
        // yield, even while incoming traffic keeps readiness continuously set.
        tokio::task::yield_now().await;
        if queue.dispatch_pending(watcher).map_err(io::Error::other)? > 0 {
            return Ok(());
        }
        // Flush also surfaces backend errors that dispatch_pending may suppress,
        // before prepare_read can ask us to dispatch its internal queue again.
        let interest = match queue.flush().map_err(wayland_io_error) {
            Ok(()) => Interest::READABLE,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                Interest::READABLE | Interest::WRITABLE
            }
            Err(error) => return Err(error),
        };
        let Some(read) = queue.prepare_read() else {
            continue;
        };
        // This connection has only one reader, so read() cannot wait on other
        // read guards. Dropping the future also cancels its prepared read.
        let mut ready = socket.ready(interest).await?;
        if ready.ready().is_writable() {
            match queue.flush().map_err(wayland_io_error) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    ready.clear_ready_matching(Ready::WRITABLE);
                }
                Err(error) => return Err(error),
            }
        }
        if ready.ready().is_readable() {
            match read.read().map_err(wayland_io_error) {
                // libwayland turns EAGAIN into success with zero dispatched
                // events. Zero can also mean a partial protocol message, so
                // retain readiness if there are still bytes in the socket.
                Ok(0) if rustix::io::ioctl_fionread(socket.get_ref())? == 0 => {
                    if ready.ready().is_read_closed() {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "Wayland connection closed during an incomplete event",
                        ));
                    }
                    ready.clear_ready_matching(Ready::READABLE);
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    ready.clear_ready_matching(Ready::READABLE);
                }
                Err(error) => return Err(error),
            }
        }
    }
}

fn wayland_io_error(error: WaylandError) -> io::Error {
    match error {
        WaylandError::Io(error) => error,
        WaylandError::Protocol(error) => io::Error::other(error),
    }
}

#[cfg(test)]
async fn watch_wayland_outputs(
    connection: Connection,
    mut send: impl FnMut(Vec<String>) -> io::Result<()>,
) -> io::Result<()> {
    watch_outputs(connection, move |outputs| {
        send(
            outputs
                .into_iter()
                .filter_map(|output| output.name)
                .collect(),
        )
    })
    .await
}

#[cfg(test)]
mod tests;
