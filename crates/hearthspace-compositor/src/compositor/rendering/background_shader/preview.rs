//! Render a candidate without replacing the desktop's retained wallpaper.
use super::*;
use crate::compositor::{CalloopData, runtime::Backend, shell_integration::ControlReply};
use smithay::backend::{
    allocator::Fourcc,
    renderer::{Bind, Color32F, Offscreen, damage::OutputDamageTracker, gles::GlesRenderbuffer},
};
use smithay::utils::Transform;

pub(in crate::compositor) fn render(
    renderer: &mut GlesRenderer,
    path: &str,
    mode: ColorMode,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut shader = BackgroundShader::default();
    let size = (336, 256);
    let element = shader
        .element(
            renderer,
            Some(path),
            ShaderView {
                output: Rectangle::from_size(size.into()),
                density: 1,
                camera: CanvasPoint { x: 0, y: 0 },
                zoom: 1.,
                mode,
            },
        )
        .ok_or("Wallpaper failed to load")?;
    if shader.error.is_some() {
        return Err("Wallpaper failed to load".into());
    }
    let mut buffer: GlesRenderbuffer = renderer.create_buffer(Fourcc::Abgr8888, size.into())?;
    let mut framebuffer = renderer.bind(&mut buffer)?;
    let mut damage = OutputDamageTracker::new(size, 1., Transform::Normal);
    damage.render_output(renderer, &mut framebuffer, 0, &[element], Color32F::BLACK)?;
    let pixels =
        crate::compositor::capture::read_pixels(renderer, &framebuffer, size.into(), false)?;
    crate::compositor::capture::encode_png(size.into(), &pixels)
}

impl CalloopData {
    pub(in crate::compositor) fn wallpaper_preview(&mut self, path: &str) -> ControlReply {
        if !self.capture_active() {
            return ControlReply::Err("Wallpaper failed to load".into());
        }
        let mode = self.state.appearance.color_mode;
        let render = |renderer: &mut GlesRenderer| render(renderer, path, mode);
        let result = match &mut self.backend {
            #[cfg(feature = "winit")]
            Backend::Winit(backend) => render(backend.renderer()),
            Backend::Headless(backend) => render(&mut backend.renderer),
            #[cfg(feature = "udev")]
            Backend::Udev(backend) => match backend.acquire_import_renderer() {
                Ok(Some(mut renderer)) => render(renderer.as_mut()),
                _ => Err("Wallpaper failed to load".into()),
            },
        };
        self.full_redraw = 3;
        self.state.request_redraw();
        result.map_or_else(
            |_| ControlReply::Err("Wallpaper failed to load".into()),
            ControlReply::Payload,
        )
    }
}
