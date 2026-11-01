use super::{
    boundary::{Image, ManualFence},
    renderer::DeviceRenderer,
};
use smithay::{
    backend::renderer::{Color32F, ContextId, Frame, Renderer, sync::SyncPoint},
    utils::{Buffer, Physical, Rectangle, Size, Transform},
};
use std::io;

#[derive(Debug)]
pub struct DeviceFrame<'a> {
    pub renderer: &'a mut DeviceRenderer,
    pub size: Size<i32, Physical>,
    pub transform: Transform,
}

impl Frame for DeviceFrame<'_> {
    type Error = io::Error;
    type TextureId = Image;

    fn context_id(&self) -> ContextId<Image> {
        self.renderer.context_id()
    }

    fn clear(&mut self, _: Color32F, _: &[Rectangle<i32, Physical>]) -> Result<(), io::Error> {
        if self.renderer.fail_draw {
            Err(io::Error::other("injected device draw failure"))
        } else {
            Ok(())
        }
    }

    fn draw_solid(
        &mut self,
        _: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        color: Color32F,
    ) -> Result<(), io::Error> {
        self.clear(color, damage)
    }

    fn render_texture_from_to(
        &mut self,
        texture: &Image,
        _: Rectangle<f64, Buffer>,
        _: Rectangle<i32, Physical>,
        _: &[Rectangle<i32, Physical>],
        _: &[Rectangle<i32, Physical>],
        _: Transform,
        _: f32,
    ) -> Result<(), io::Error> {
        if self.renderer.fail_draw {
            return Err(io::Error::other("injected device sample failure"));
        }
        if !self.renderer.source {
            let mut stats = self.renderer.stats.lock().unwrap();
            if let Some(buffer) = &texture.buffer {
                stats.direct_samples.push(buffer.weak());
            } else {
                stats.memory_samples += 1;
            }
        }
        Ok(())
    }

    fn transformation(&self) -> Transform {
        self.transform
    }

    fn output_size(&self) -> Size<i32, Physical> {
        self.size
    }

    fn wait(&mut self, sync: &SyncPoint) -> Result<(), io::Error> {
        self.renderer.wait(sync)
    }

    fn finish(self) -> Result<SyncPoint, io::Error> {
        if self.renderer.fail_finish {
            return Err(io::Error::other("injected device finish failure"));
        }
        if self.renderer.source {
            Ok(SyncPoint::signaled())
        } else {
            let fence = ManualFence::default();
            self.renderer
                .stats
                .lock()
                .unwrap()
                .completions
                .push(fence.clone());
            Ok(SyncPoint::from(fence))
        }
    }
}
