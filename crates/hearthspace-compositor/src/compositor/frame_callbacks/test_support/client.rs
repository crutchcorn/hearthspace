use std::collections::HashMap;

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool,
        wl_subcompositor, wl_subsurface, wl_surface,
    },
};
use wayland_protocols::{
    ext::session_lock::v1::client::{
        ext_session_lock_manager_v1, ext_session_lock_surface_v1, ext_session_lock_v1,
    },
    xdg::shell::client::{xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base},
};

#[derive(Default)]
pub(super) struct ClientState {
    pub(super) compositor: Option<wl_compositor::WlCompositor>,
    pub(super) subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    pub(super) xdg: Option<xdg_wm_base::XdgWmBase>,
    pub(super) shm: Option<wl_shm::WlShm>,
    pub(super) lock_manager: Option<ext_session_lock_manager_v1::ExtSessionLockManagerV1>,
    pub(super) outputs: HashMap<String, wl_output::WlOutput>,
    output_bindings: Vec<wl_output::WlOutput>,
    pub(super) done: Vec<(usize, u32)>,
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
                "xdg_wm_base" => state.xdg = Some(registry.bind(name, version.min(6), queue, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, 1, queue, ())),
                "ext_session_lock_manager_v1" => {
                    state.lock_manager = Some(registry.bind(name, 1, queue, ()))
                }
                "wl_output" => state
                    .output_bindings
                    .push(registry.bind(name, 4, queue, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_callback::WlCallback, Option<usize>> for ClientState {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        surface: &Option<usize>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { callback_data } = event {
            if let Some(surface) = surface {
                state.done.push((*surface, callback_data));
            } else {
                state.synced = true;
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for ClientState {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            state.outputs.insert(name, output.clone());
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for ClientState {
    fn event(
        _: &mut Self,
        base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for ClientState {
    fn event(
        _: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
        }
    }
}

impl Dispatch<ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, ()> for ClientState {
    fn event(
        _: &mut Self,
        surface: &ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_surface_v1::Event::Configure { serial, .. } = event {
            surface.ack_configure(serial);
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
delegate_noop!(ClientState: ignore xdg_positioner::XdgPositioner);
delegate_noop!(ClientState: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(ClientState: ignore xdg_popup::XdgPopup);
delegate_noop!(ClientState: ignore ext_session_lock_manager_v1::ExtSessionLockManagerV1);
delegate_noop!(ClientState: ignore ext_session_lock_v1::ExtSessionLockV1);
