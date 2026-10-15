use super::{Backend, CalloopData, encode_png, read_pixels};
use smithay::{
    backend::renderer::{Bind, damage::OutputDamageTracker},
    utils::Transform,
};

impl CalloopData {
    /// Existing nested/headless PNG contract. Native captures must be deferred.
    pub(in crate::compositor) fn screenshot_png(
        &mut self,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        self.process_pending_dmabuf_imports();
        let Self { state, backend, .. } = self;
        let size = state.output_size();
        let mut damage = OutputDamageTracker::new(size, 1.0, Transform::Flipped180);
        let pixels = match backend {
            #[cfg(feature = "winit")]
            Backend::Winit(backend) => {
                let (renderer, mut framebuffer) = backend.bind()?;
                let software_cursor = std::mem::replace(&mut state.software_cursor_visible, true);
                let rendered = state.render_frame(renderer, &mut framebuffer, &mut damage, 0);
                state.software_cursor_visible = software_cursor;
                rendered?;
                read_pixels(renderer, &framebuffer, size, true)?
            }
            Backend::Headless(backend) => {
                let mut framebuffer = backend.renderer.bind(&mut backend.buffer)?;
                state.render_frame(&mut backend.renderer, &mut framebuffer, &mut damage, 0)?;
                read_pixels(&mut backend.renderer, &framebuffer, size, true)?
            }
            #[cfg(feature = "udev")]
            Backend::Udev(_) => {
                return Err("native captures require the asynchronous presentation path".into());
            }
        };
        encode_png(size, &pixels)
    }
}
