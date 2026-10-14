use std::io;

use tracing::error;
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{wl_output, wl_registry},
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
            let result = xilem::tokio::task::spawn_blocking(move || watch_wayland_outputs(proxy))
                .await
                .unwrap_or_else(|error| Err(io::Error::other(error)));
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

fn watch_wayland_outputs(proxy: MessageProxy<OutputNamesChanged>) -> io::Result<()> {
    let connection = Connection::connect_to_env().map_err(io::Error::other)?;
    let mut event_queue = connection.new_event_queue();
    let queue_handle = event_queue.handle();
    let _registry = connection.display().get_registry(&queue_handle, ());
    let mut watcher = OutputWatcher::default();
    event_queue
        .roundtrip(&mut watcher)
        .map_err(io::Error::other)?;
    event_queue
        .roundtrip(&mut watcher)
        .map_err(io::Error::other)?;
    watcher.take_changed();
    send_output_names(&proxy, &watcher)?;

    loop {
        event_queue
            .blocking_dispatch(&mut watcher)
            .map_err(io::Error::other)?;
        if watcher.take_changed() {
            send_output_names(&proxy, &watcher)?;
        }
    }
}

fn send_output_names(
    proxy: &MessageProxy<OutputNamesChanged>,
    watcher: &OutputWatcher,
) -> io::Result<()> {
    proxy
        .message(OutputNamesChanged {
            names: watcher.output_names(),
        })
        .map_err(|error| io::Error::other(error.to_string()))
}

#[derive(Default)]
struct OutputWatcher {
    outputs: Vec<OutputState>,
    changed: bool,
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
