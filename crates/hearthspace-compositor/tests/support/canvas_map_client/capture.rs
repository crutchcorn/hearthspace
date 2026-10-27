//! Extra protocol operations used by capture regressions.
use super::*;
use std::{io::Write, os::fd::AsFd};
use wayland_protocols::xdg::shell::client::{xdg_popup, xdg_positioner};

#[derive(Clone)]
struct PopupData {
    surface: wl_surface::WlSurface,
    size: (i32, i32),
    color: [u8; 4],
}

impl MapClient {
    pub(in super::super) fn capture_connection(&self) -> Connection {
        self._connection.clone()
    }

    pub(in super::super) fn capture_title(&mut self, number: usize, title: &str) {
        self.state.windows[number].toplevel.set_title(title.into());
        self.sync();
    }

    pub(in super::super) fn capture_app_id(&mut self, number: usize, app_id: &str) {
        self.state.windows[number]
            .toplevel
            .set_app_id(app_id.into());
        self.sync();
    }

    pub(in super::super) fn capture_scale(&mut self, number: usize, scale: i32) {
        let window = &self.state.windows[number];
        let width = window.size.0 + window.inset.0 * 2;
        let height = window.size.1 + window.inset.1 * 2;
        let native = pixels::paint(window);
        let mut bytes = Vec::new();
        for y in 0..height * scale {
            for x in 0..width * scale {
                let offset = ((y / scale * width + x / scale) * 4) as usize;
                bytes.extend_from_slice(&native[offset..offset + 4]);
            }
        }
        let (file, buffer) = shm_buffer(
            &self.state,
            &self.queue.handle(),
            (width * scale, height * scale),
            &bytes,
        );
        window.surface.set_buffer_scale(scale);
        window.surface.attach(Some(&buffer), 0, 0);
        window
            .surface
            .damage_buffer(0, 0, width * scale, height * scale);
        window.surface.commit();
        self.state.buffers.push((file, buffer));
        self.sync();
    }

    pub(in super::super) fn capture_popup(
        &mut self,
        number: usize,
        origin: (i32, i32),
        size: (i32, i32),
        color: [u8; 4],
    ) -> (xdg_popup::XdgPopup, wl_surface::WlSurface) {
        let q = self.queue.handle();
        let surface = self
            .state
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&q, ());
        let wm = self.state.wm_base.as_ref().unwrap();
        let xdg = wm.get_xdg_surface(
            &surface,
            &q,
            PopupData {
                surface: surface.clone(),
                size,
                color,
            },
        );
        let positioner = wm.create_positioner(&q, ());
        positioner.set_size(size.0, size.1);
        positioner.set_anchor_rect(origin.0 - 1, origin.1 - 1, 1, 1);
        positioner.set_anchor(xdg_positioner::Anchor::BottomRight);
        positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
        let popup = xdg.get_popup(
            Some(&self.state.windows[number].xdg_surface),
            &positioner,
            &q,
            (),
        );
        positioner.destroy();
        surface.commit();
        self.sync();
        (popup, surface)
    }
}

pub(super) fn shm_buffer(
    state: &State,
    q: &QueueHandle<State>,
    size: (i32, i32),
    bytes: &[u8],
) -> (File, wl_buffer::WlBuffer) {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(bytes).unwrap();
    let pool = state
        .shm
        .as_ref()
        .unwrap()
        .create_pool(file.as_fd(), bytes.len() as i32, q, ());
    let buffer = pool.create_buffer(
        0,
        size.0,
        size.1,
        size.0 * 4,
        wl_shm::Format::Argb8888,
        q,
        (),
    );
    pool.destroy();
    (file, buffer)
}

impl Dispatch<xdg_surface::XdgSurface, PopupData> for State {
    fn event(
        state: &mut Self,
        object: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        data: &PopupData,
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            object.ack_configure(serial);
            let bytes = data.color.repeat((data.size.0 * data.size.1) as usize);
            let (file, buffer) = shm_buffer(state, q, data.size, &bytes);
            data.surface.attach(Some(&buffer), 0, 0);
            data.surface.damage_buffer(0, 0, data.size.0, data.size.1);
            data.surface.commit();
            state.buffers.push((file, buffer));
        }
    }
}

delegate_noop!(State: ignore xdg_popup::XdgPopup);
delegate_noop!(State: ignore xdg_positioner::XdgPositioner);
