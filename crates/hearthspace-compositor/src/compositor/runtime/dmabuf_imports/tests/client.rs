use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{wl_buffer, wl_callback, wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_buffer_params_v1, zwp_linux_dmabuf_v1,
};

#[derive(Default)]
pub(super) struct ClientState {
    pub(super) dmabuf: Option<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1>,
    pub(super) compositor: Option<wl_compositor::WlCompositor>,
    pub(super) created: Vec<(usize, wl_buffer::WlBuffer)>,
    pub(super) failed: Vec<usize>,
    pub(super) synced: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for ClientState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "zwp_linux_dmabuf_v1" => {
                    state.dmabuf = Some(registry.bind(name, version.min(3), queue, ()))
                }
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(5), queue, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1, usize> for ClientState {
    fn event(
        state: &mut Self,
        _: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        event: zwp_linux_buffer_params_v1::Event,
        id: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_linux_buffer_params_v1::Event::Created { buffer } => {
                state.created.push((*id, buffer));
            }
            zwp_linux_buffer_params_v1::Event::Failed => state.failed.push(*id),
            _ => {}
        }
    }

    wayland_client::event_created_child!(ClientState, zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1, [
        0 => (wl_buffer::WlBuffer, ())
    ]);
}

impl Dispatch<wl_callback::WlCallback, ()> for ClientState {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.synced = true;
    }
}

delegate_noop!(ClientState: ignore zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1);
delegate_noop!(ClientState: ignore wl_compositor::WlCompositor);
delegate_noop!(ClientState: ignore wl_surface::WlSurface);
delegate_noop!(ClientState: ignore wl_buffer::WlBuffer);
