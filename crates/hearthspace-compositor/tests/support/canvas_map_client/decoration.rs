//! Observe negotiated decorations before the first native surface buffer.
use std::collections::BTreeMap;

use super::{Connection, Dispatch, MapClient, QueueHandle, delegate_noop, wl_registry};
use wayland_client::WEnum;
use wayland_protocols::xdg::{
    decoration::zv1::client::{zxdg_decoration_manager_v1, zxdg_toplevel_decoration_v1},
    shell::client::xdg_toplevel,
};

type Mode = zxdg_toplevel_decoration_v1::Mode;

#[derive(Default)]
pub(super) struct State {
    manager: Option<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1>,
    next: Option<Mode>,
    objects: BTreeMap<usize, zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1>,
    modes: BTreeMap<usize, Mode>,
}

impl State {
    pub(super) fn destroy(&mut self, number: usize) {
        if let Some(decoration) = self.objects.remove(&number) {
            decoration.destroy();
        }
        self.modes.remove(&number);
    }

    pub(super) fn bind(
        &mut self,
        registry: &wl_registry::WlRegistry,
        name: u32,
        handle: &QueueHandle<super::State>,
    ) {
        self.manager = Some(registry.bind(name, 1, handle, ()));
    }

    pub(super) fn create(
        &mut self,
        toplevel: &xdg_toplevel::XdgToplevel,
        handle: &QueueHandle<super::State>,
        number: usize,
    ) {
        if let Some(mode) = self.next.take() {
            let decoration = self
                .manager
                .as_ref()
                .unwrap()
                .get_toplevel_decoration(toplevel, handle, number);
            decoration.set_mode(mode);
            self.objects.insert(number, decoration);
        }
    }
}

impl MapClient {
    pub(in super::super) fn add_decorated(
        &mut self,
        app_id: String,
        size: (i32, i32),
        color: [u8; 4],
        mode: Mode,
    ) -> usize {
        self.state.decorations.next = Some(mode);
        self.add(app_id, size, (0, 0), color)
    }

    pub(in super::super) fn decoration_mode(&mut self, number: usize) -> Mode {
        self.sync();
        self.state.decorations.modes[&number]
    }

    pub(in super::super) fn request_decoration(&mut self, number: usize, mode: Mode) {
        self.state.decorations.objects[&number].set_mode(mode);
        self.state.windows[number].surface.commit();
        self.sync();
    }
}

impl Dispatch<zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1, usize> for super::State {
    fn event(
        state: &mut Self,
        _: &zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1,
        event: zxdg_toplevel_decoration_v1::Event,
        number: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_toplevel_decoration_v1::Event::Configure {
            mode: WEnum::Value(mode),
        } = event
        {
            state.decorations.modes.insert(*number, mode);
        }
    }
}

delegate_noop!(super::State: ignore zxdg_decoration_manager_v1::ZxdgDecorationManagerV1);
