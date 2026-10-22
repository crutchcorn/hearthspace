use std::{
    io,
    os::fd::{AsFd, BorrowedFd},
    path::PathBuf,
};

use tokio::io::{Interest, Ready, unix::AsyncFd};
use tracing::error;
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle,
    backend::WaylandError,
    protocol::{wl_callback, wl_output, wl_registry},
};
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

use super::state::{OutputNamesChanged, ShellState};

pub(super) fn output_watcher_task()
-> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        move |proxy: MessageProxy<OutputNamesChanged>, _state: &mut ShellState| async move {
            let result = async {
                let connection = connect_wayland().await?;
                watch_wayland_outputs(connection, |names| {
                    proxy
                        .message(OutputNamesChanged { names })
                        .map_err(|error| io::Error::other(error.to_string()))
                })
                .await
            }
            .await;
            if let Err(error) = result {
                error!(%error, "Wayland output watcher stopped");
            }
        },
        |state: &mut ShellState, message: OutputNamesChanged| {
            state.apply_output_names(message.names);
        },
    )
}

pub(super) fn initial_wayland_output_names() -> io::Result<Vec<String>> {
    let connection = Connection::connect_to_env().map_err(io::Error::other)?;
    let mut event_queue = connection.new_event_queue();
    let queue_handle = event_queue.handle();
    let _registry = connection.display().get_registry(&queue_handle, ());
    let mut watcher = OutputWatcher::default();
    event_queue
        .roundtrip(&mut watcher)
        .map_err(io::Error::other)?;
    // Registry dispatch binds wl_output objects. Their initial name events only
    // arrive after those bind requests reach the compositor on a second trip.
    event_queue
        .roundtrip(&mut watcher)
        .map_err(io::Error::other)?;
    Ok(watcher.output_names())
}

async fn connect_wayland() -> io::Result<Connection> {
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

async fn watch_wayland_outputs(
    connection: Connection,
    mut send_names: impl FnMut(Vec<String>) -> io::Result<()>,
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
    send_names(watcher.output_names())?;

    loop {
        dispatch_outputs(&mut event_queue, &socket, &mut watcher).await?;
        if watcher.take_changed() {
            send_names(watcher.output_names())?;
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

#[derive(Default)]
struct OutputWatcher {
    outputs: Vec<OutputState>,
    changed: bool,
    sync_done: bool,
}

impl Dispatch<wl_callback::WlCallback, ()> for OutputWatcher {
    fn event(
        state: &mut Self,
        _proxy: &wl_callback::WlCallback,
        _event: wl_callback::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        state.sync_done = true;
    }
}

struct OutputState {
    global_name: u32,
    _output: wl_output::WlOutput,
    name: Option<String>,
}

struct OutputGlobal {
    global_name: u32,
}

impl OutputWatcher {
    fn bind_output(
        &mut self,
        registry: &wl_registry::WlRegistry,
        global_name: u32,
        version: u32,
        queue_handle: &QueueHandle<Self>,
    ) {
        if self
            .outputs
            .iter()
            .any(|output| output.global_name == global_name)
        {
            return;
        }

        let output = registry.bind::<wl_output::WlOutput, _, _>(
            global_name,
            version.min(4),
            queue_handle,
            OutputGlobal { global_name },
        );
        self.outputs.push(OutputState {
            global_name,
            _output: output,
            name: None,
        });
    }

    fn output_mut(&mut self, global_name: u32) -> Option<&mut OutputState> {
        self.outputs
            .iter_mut()
            .find(|output| output.global_name == global_name)
    }

    fn remove_output(&mut self, global_name: u32) {
        let original_len = self.outputs.len();
        self.outputs
            .retain(|output| output.global_name != global_name);
        self.changed |= self.outputs.len() != original_len;
    }

    fn output_names(&self) -> Vec<String> {
        self.outputs
            .iter()
            .filter_map(|output| output.name.clone())
            .collect()
    }

    fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for OutputWatcher {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _connection: &Connection,
        queue_handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == "wl_output" => {
                state.bind_output(registry, name, version, queue_handle);
            }
            wl_registry::Event::GlobalRemove { name } => state.remove_output(name),
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, OutputGlobal> for OutputWatcher {
    fn event(
        state: &mut Self,
        _proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        data: &OutputGlobal,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        let Some(output) = state.output_mut(data.global_name) else {
            return;
        };
        if let wl_output::Event::Name { name } = event
            && output.name.as_deref() != Some(&name)
        {
            output.name = Some(name);
            state.changed = true;
        }
    }
}

#[cfg(test)]
mod tests;
