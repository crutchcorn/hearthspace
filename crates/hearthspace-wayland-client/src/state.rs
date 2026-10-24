use crate::OutputInfo;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
    protocol::{wl_callback, wl_output, wl_registry},
};

#[derive(Default)]
pub(crate) struct OutputWatcher {
    outputs: Vec<OutputState>,
    changed: bool,
    pub(crate) sync_done: bool,
}

struct OutputState {
    output: wl_output::WlOutput,
    pending: OutputInfo,
    committed: Option<OutputInfo>,
}

struct OutputGlobal {
    name: u32,
}

impl OutputWatcher {
    fn bind_output(
        &mut self,
        registry: &wl_registry::WlRegistry,
        name: u32,
        version: u32,
        handle: &QueueHandle<Self>,
    ) {
        if self
            .outputs
            .iter()
            .any(|output| output.pending.global_name == name)
        {
            return;
        }
        let output = registry.bind::<wl_output::WlOutput, _, _>(
            name,
            version.min(4),
            handle,
            OutputGlobal { name },
        );
        self.outputs.push(OutputState {
            output,
            pending: OutputInfo {
                global_name: name,
                name: None,
                description: None,
                position: None,
                size: None,
                refresh_millihertz: None,
                scale: 1,
            },
            committed: None,
        });
    }

    fn remove_output(&mut self, name: u32) {
        let count = self.outputs.len();
        self.outputs
            .retain(|output| output.pending.global_name != name);
        self.changed |= count != self.outputs.len();
    }

    pub(crate) fn snapshot(&self) -> Vec<OutputInfo> {
        self.outputs
            .iter()
            .filter_map(|output| output.committed.clone())
            .collect()
    }

    pub(crate) fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for OutputWatcher {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.sync_done = true;
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for OutputWatcher {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == "wl_output" => state.bind_output(registry, name, version, handle),
            wl_registry::Event::GlobalRemove { name } => state.remove_output(name),
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, OutputGlobal> for OutputWatcher {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        data: &OutputGlobal,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state
            .outputs
            .iter_mut()
            .find(|output| output.pending.global_name == data.name)
        else {
            return;
        };
        let done = matches!(event, wl_output::Event::Done);
        match event {
            wl_output::Event::Geometry { x, y, .. } => output.pending.position = Some((x, y)),
            wl_output::Event::Name { name } => output.pending.name = Some(name),
            wl_output::Event::Description { description } => {
                output.pending.description = Some(description)
            }
            wl_output::Event::Mode {
                flags: WEnum::Value(flags),
                width,
                height,
                refresh,
            } if flags.contains(wl_output::Mode::Current) && width > 0 && height > 0 => {
                output.pending.size = Some((width, height));
                output.pending.refresh_millihertz = (refresh > 0).then_some(refresh);
            }
            wl_output::Event::Scale { factor } if factor > 0 => output.pending.scale = factor,
            _ => {}
        }
        if (done || output.output.version() == 1)
            && output.committed.as_ref() != Some(&output.pending)
        {
            output.committed = Some(output.pending.clone());
            state.changed = true;
        }
    }
}
