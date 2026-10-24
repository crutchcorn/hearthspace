//! Separate configure acknowledgment from the buffer that adopts its size.
use super::MapClient;

impl MapClient {
    pub(in super::super) fn unmap(&mut self, number: usize) {
        let window = &mut self.state.windows[number];
        // Acknowledge any fresh configure without automatically mapping again.
        window.defer_buffer = true;
        window.surface.attach(None, 0, 0);
        window.surface.commit();
        self.sync();
    }

    pub(in super::super) fn remap(&mut self, number: usize, app_id: &str) {
        // Unmap's barriers acknowledge the fresh initial configure. Restore
        // role properties before attaching to the same surface and toplevel.
        let window = &mut self.state.windows[number];
        window.toplevel.set_app_id(app_id.into());
        window.toplevel.set_min_size(window.size.0, window.size.1);
        window.toplevel.set_max_size(window.size.0, window.size.1);
        window.defer_buffer = false;
        self.state.commit_buffer(number, &self.queue.handle());
        self.sync();
    }

    pub(in super::super) fn begin_resize(&mut self, number: usize, size: (i32, i32)) {
        self.state.windows[number].defer_buffer = true;
        self.resize(number, size);
    }

    pub(in super::super) fn commit_resize(&mut self, number: usize) {
        self.state.windows[number].defer_buffer = false;
        self.state.commit_buffer(number, &self.queue.handle());
        self.sync();
    }

    pub(in super::super) fn commit_old_buffer(&mut self, number: usize) {
        self.state.windows[number].surface.commit();
        self.sync();
    }

    pub(in super::super) fn commit_geometry_without_buffer(&mut self, number: usize) {
        let window = &self.state.windows[number];
        window
            .xdg_surface
            .set_window_geometry(0, 0, window.size.0, window.size.1);
        window.surface.commit();
        self.sync();
    }

    pub(in super::super) fn pointer_target(&mut self) -> Option<usize> {
        self.sync();
        self.state.pointer
    }

    pub(in super::super) fn pointer_position(&mut self) -> (f64, f64) {
        self.sync();
        self.state.pointer_xy
    }
}
