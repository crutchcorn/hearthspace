//! Chrome-style dropdown: a desynchronized subsurface within a toplevel.
use super::*;
use wayland_client::protocol::{wl_subcompositor, wl_subsurface};

pub(super) struct Subsurface {
    surface: wl_surface::WlSurface,
    role: wl_subsurface::WlSubsurface,
}

impl InteractiveClient {
    pub(crate) fn open_subsurface(&mut self) {
        let h = self.queue.handle();
        let surface = self
            .state
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&h, ());
        let role = self.state.subcompositor.as_ref().unwrap().get_subsurface(
            &surface,
            &self.state.windows[0].surface,
            &h,
            (),
        );
        role.set_position(40, 30);
        role.set_desync();
        let mut file = tempfile::tempfile().unwrap();
        file.set_len(120 * 80 * 4).unwrap();
        file.write_all(&[0x00, 0xcc, 0x66, 0xff].repeat(120 * 80))
            .unwrap();
        let pool = self
            .state
            .shm
            .as_ref()
            .unwrap()
            .create_pool(file.as_fd(), 120 * 80 * 4, &h, ());
        let buffer = pool.create_buffer(0, 120, 80, 480, wl_shm::Format::Argb8888, &h, ());
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, 120, 80);
        surface.commit();
        self.state.windows[0].surface.commit();
        pool.destroy();
        self.state.buffers.push((file, buffer));
        self.state.subsurface = Some(Subsurface { surface, role });
        self.sync();
    }

    pub(crate) fn subsurface_surface(&self) -> &wl_surface::WlSurface {
        &self.state.subsurface.as_ref().unwrap().surface
    }
}

impl Drop for Subsurface {
    fn drop(&mut self) {
        self.role.destroy();
        self.surface.destroy();
    }
}

delegate_noop!(State: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(State: ignore wl_subsurface::WlSubsurface);
