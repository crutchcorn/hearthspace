use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface,
    },
};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};

#[derive(Default)]
pub(super) struct ClientState {
    pub(super) compositor: Option<wl_compositor::WlCompositor>,
    pub(super) subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    pub(super) shm: Option<wl_shm::WlShm>,
    pub(super) viewporter: Option<wp_viewporter::WpViewporter>,
    pub(super) pointer: Option<wl_pointer::WlPointer>,
    pub(super) enter_serial: Option<u32>,
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
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(5), queue, ()))
                }
                "wl_subcompositor" => state.subcompositor = Some(registry.bind(name, 1, queue, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, 1, queue, ())),
                "wp_viewporter" => state.viewporter = Some(registry.bind(name, 1, queue, ())),
                "wl_seat" => {
                    let seat: wl_seat::WlSeat = registry.bind(name, version.min(7), queue, ());
                    state.pointer = Some(seat.get_pointer(queue, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for ClientState {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Enter { serial, .. } = event {
            state.enter_serial = Some(serial);
        }
    }
}

impl Dispatch<wl_callback::WlCallback, bool> for ClientState {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        sync: &bool,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if *sync {
            state.synced = true;
        }
    }
}

delegate_noop!(ClientState: ignore wl_compositor::WlCompositor);
delegate_noop!(ClientState: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(ClientState: ignore wl_subsurface::WlSubsurface);
delegate_noop!(ClientState: ignore wl_surface::WlSurface);
delegate_noop!(ClientState: ignore wl_shm::WlShm);
delegate_noop!(ClientState: ignore wl_shm_pool::WlShmPool);
delegate_noop!(ClientState: ignore wl_buffer::WlBuffer);
delegate_noop!(ClientState: ignore wl_seat::WlSeat);
delegate_noop!(ClientState: ignore wp_viewporter::WpViewporter);
delegate_noop!(ClientState: ignore wp_viewport::WpViewport);
