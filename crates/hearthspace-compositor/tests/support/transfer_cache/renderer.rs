use super::{
    boundary::{Image, SharedStats, context},
    frame::DeviceFrame,
};
use smithay::{
    backend::{
        allocator::{Fourcc, dmabuf::Dmabuf},
        renderer::{
            Bind, ContextId, DebugFlags, Renderer, RendererSuper, TextureFilter, sync::SyncPoint,
        },
    },
    utils::{Physical, Size, Transform},
};
use std::io;

#[derive(Debug)]
pub struct DeviceRenderer {
    pub source: bool,
    pub context: ContextId<Image>,
    pub stats: SharedStats,
    pub reject_dma: bool,
    pub fail_draw: bool,
    pub fail_finish: bool,
    pub fail_render: bool,
    pub fail_readback: bool,
    flags: DebugFlags,
}

impl DeviceRenderer {
    pub fn new(source: bool, stats: SharedStats) -> Self {
        Self {
            source,
            context: context(),
            stats,
            reject_dma: false,
            fail_draw: false,
            fail_finish: false,
            fail_render: false,
            fail_readback: false,
            flags: DebugFlags::empty(),
        }
    }
}

impl RendererSuper for DeviceRenderer {
    type Error = io::Error;
    type TextureId = Image;
    type Framebuffer<'buffer> = Image;
    type Frame<'frame, 'buffer>
        = DeviceFrame<'frame>
    where
        'buffer: 'frame,
        Self: 'frame;
}

impl Renderer for DeviceRenderer {
    fn context_id(&self) -> ContextId<Image> {
        self.context.clone()
    }

    fn downscale_filter(&mut self, _: TextureFilter) -> Result<(), io::Error> {
        Ok(())
    }

    fn upscale_filter(&mut self, _: TextureFilter) -> Result<(), io::Error> {
        Ok(())
    }

    fn set_debug_flags(&mut self, flags: DebugFlags) {
        self.flags = flags;
    }

    fn debug_flags(&self) -> DebugFlags {
        self.flags
    }

    fn render<'frame, 'buffer>(
        &'frame mut self,
        framebuffer: &'frame mut Image,
        size: Size<i32, Physical>,
        transform: Transform,
    ) -> Result<DeviceFrame<'frame>, io::Error>
    where
        'buffer: 'frame,
    {
        if self.fail_render {
            return Err(io::Error::other("injected device render failure"));
        }
        if self.source {
            self.stats
                .lock()
                .unwrap()
                .source_writes
                .push(framebuffer.buffer.as_ref().unwrap().weak());
        }
        assert_eq!((framebuffer.size.w, framebuffer.size.h), (size.w, size.h));
        Ok(DeviceFrame {
            renderer: self,
            size,
            transform,
        })
    }

    fn wait(&mut self, sync: &SyncPoint) -> Result<(), io::Error> {
        sync.wait().map_err(io::Error::other)
    }
}

impl Bind<Dmabuf> for DeviceRenderer {
    fn bind<'buffer>(&mut self, buffer: &'buffer mut Dmabuf) -> Result<Image, io::Error> {
        Ok(Image::from_buffer(buffer))
    }

    fn supported_formats(&self) -> Option<smithay::backend::allocator::format::FormatSet> {
        Some(super::memory::formats())
    }
}

pub const FORMAT: Fourcc = Fourcc::Argb8888;
