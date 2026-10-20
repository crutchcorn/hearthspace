use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_output, wl_registry},
};

use crate::monitors::{EMPTY_MONITOR_CANVAS_HEIGHT, EMPTY_MONITOR_CANVAS_WIDTH, LiveOutput};

pub(crate) fn live_wayland_outputs() -> std::io::Result<Vec<LiveOutput>> {
    let connection = Connection::connect_to_env().map_err(std::io::Error::other)?;
    let (globals, mut event_queue) =
        registry_queue_init::<OutputProbe>(&connection).map_err(std::io::Error::other)?;
    let queue_handle = event_queue.handle();
    let mut probe = OutputProbe::default();
    for global in globals.contents().clone_list() {
        if global.interface == "wl_output" {
            globals.registry().bind::<wl_output::WlOutput, _, _>(
                global.name,
                global.version.min(4),
                &queue_handle,
                OutputGlobal { name: global.name },
            );
        }
    }
    event_queue
        .roundtrip(&mut probe)
        .map_err(std::io::Error::other)?;
    Ok(probe.live_outputs())
}

#[derive(Default)]
struct OutputProbe {
    outputs: Vec<OutputState>,
}

struct OutputGlobal {
    name: u32,
}

#[derive(Debug)]
struct OutputState {
    global_name: u32,
    name: Option<String>,
    description: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    refresh_millihertz: Option<i32>,
    scale: Option<i32>,
}

impl OutputState {
    fn current_mode(
        &mut self,
        flags: WEnum<wl_output::Mode>,
        width: i32,
        height: i32,
        refresh: i32,
    ) {
        if output_size_is_current(flags) {
            self.width = Some(width);
            self.height = Some(height);
            self.refresh_millihertz = (refresh > 0).then_some(refresh);
        }
    }
}

impl OutputProbe {
    fn output_mut(&mut self, global_name: u32) -> &mut OutputState {
        if let Some(index) = self
            .outputs
            .iter()
            .position(|output| output.global_name == global_name)
        {
            return &mut self.outputs[index];
        }
        self.outputs.push(OutputState {
            global_name,
            name: None,
            description: None,
            width: None,
            height: None,
            refresh_millihertz: None,
            scale: None,
        });
        self.outputs.last_mut().unwrap()
    }

    fn live_outputs(self) -> Vec<LiveOutput> {
        self.outputs
            .into_iter()
            .map(|output| {
                let fallback = format!("output-{}", output.global_name);
                let id = output.name.clone().unwrap_or_else(|| fallback.clone());
                LiveOutput {
                    id: id.clone(),
                    name: output.description.or(output.name).unwrap_or(id),
                    width: output.width.unwrap_or(EMPTY_MONITOR_CANVAS_WIDTH),
                    height: output.height.unwrap_or(EMPTY_MONITOR_CANVAS_HEIGHT),
                    scale: output.scale.unwrap_or(1).max(1),
                    refresh_millihertz: output.refresh_millihertz,
                }
            })
            .collect()
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for OutputProbe {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_output::WlOutput, OutputGlobal> for OutputProbe {
    fn event(
        state: &mut Self,
        _proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        data: &OutputGlobal,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        let output = state.output_mut(data.name);
        match event {
            wl_output::Event::Name { name } => output.name = Some(name),
            wl_output::Event::Description { description } => {
                output.description = Some(description);
            }
            wl_output::Event::Mode {
                flags,
                width,
                height,
                refresh,
            } => output.current_mode(flags, width, height, refresh),
            wl_output::Event::Scale { factor } => output.scale = Some(factor.max(1)),
            _ => {}
        }
    }
}

fn output_size_is_current(flags: WEnum<wl_output::Mode>) -> bool {
    matches!(flags, WEnum::Value(flags) if flags.contains(wl_output::Mode::Current))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_rate_comes_only_from_the_advertised_current_mode() {
        let mut probe = OutputProbe::default();
        let output = probe.output_mut(3);
        output.current_mode(WEnum::Value(wl_output::Mode::Current), 1920, 1080, 59_940);
        output.current_mode(
            WEnum::Value(wl_output::Mode::Preferred),
            2560,
            1440,
            144_000,
        );
        let output = &probe.live_outputs()[0];
        assert_eq!((output.width, output.height), (1920, 1080));
        assert_eq!(output.refresh_millihertz, Some(59_940));
    }

    #[test]
    fn unreported_frequency_is_not_invented() {
        let mut probe = OutputProbe::default();
        probe
            .output_mut(3)
            .current_mode(WEnum::Value(wl_output::Mode::Current), 1920, 1080, 0);
        assert_eq!(probe.live_outputs()[0].refresh_millihertz, None);
    }
}
