use super::{boundary::Image, renderer::DeviceRenderer};
use smithay::{
    backend::{
        allocator::{Format, Fourcc, Modifier, dmabuf::Dmabuf, format::FormatSet},
        renderer::{ExportMem, ImportDma, ImportMem, TextureMapping},
    },
    utils::{Buffer, Rectangle, Size},
};
use std::io;

pub fn formats() -> FormatSet {
    [Fourcc::Argb8888, Fourcc::Xrgb8888, Fourcc::Abgr8888]
        .into_iter()
        .map(|code| Format {
            code,
            modifier: Modifier::Linear,
        })
        .collect()
}

impl ImportDma for DeviceRenderer {
    fn dmabuf_formats(&self) -> FormatSet {
        formats()
    }

    fn import_dmabuf(
        &mut self,
        buffer: &Dmabuf,
        _: Option<&[Rectangle<i32, Buffer>]>,
    ) -> Result<Image, io::Error> {
        self.stats.lock().unwrap().dma_imports.push(buffer.weak());
        if self.reject_dma {
            return Err(io::Error::other("injected cross-device import rejection"));
        }
        Ok(Image::from_buffer(buffer))
    }
}

impl ImportMem for DeviceRenderer {
    fn import_memory(
        &mut self,
        data: &[u8],
        format: Fourcc,
        size: Size<i32, Buffer>,
        _: bool,
    ) -> Result<Image, io::Error> {
        assert_eq!(data.len(), size.w as usize * size.h as usize * 4);
        self.stats.lock().unwrap().uploads += 1;
        Ok(Image {
            size,
            format,
            buffer: None,
            pixels: data.to_vec(),
        })
    }

    fn update_memory(
        &mut self,
        _: &Image,
        _: &[u8],
        _: Rectangle<i32, Buffer>,
    ) -> Result<(), io::Error> {
        panic!("frame transfer must import each downloaded damage region")
    }

    fn mem_formats(&self) -> Box<dyn Iterator<Item = Fourcc>> {
        Box::new([Fourcc::Argb8888, Fourcc::Xrgb8888, Fourcc::Abgr8888].into_iter())
    }
}

impl TextureMapping for Image {
    fn flipped(&self) -> bool {
        false
    }
}

impl ExportMem for DeviceRenderer {
    type TextureMapping = Image;

    fn copy_framebuffer(
        &mut self,
        _: &Image,
        region: Rectangle<i32, Buffer>,
        format: Fourcc,
    ) -> Result<Image, io::Error> {
        if self.fail_readback {
            return Err(io::Error::other("injected device readback failure"));
        }
        self.stats.lock().unwrap().downloads += 1;
        Ok(Image {
            size: region.size,
            format,
            buffer: None,
            pixels: vec![0x7f; region.size.w as usize * region.size.h as usize * 4],
        })
    }

    fn copy_texture(
        &mut self,
        _: &Image,
        _: Rectangle<i32, Buffer>,
        _: Fourcc,
    ) -> Result<Image, io::Error> {
        panic!("frame transfer must download the intermediate framebuffer")
    }

    fn can_read_texture(&mut self, _: &Image) -> Result<bool, io::Error> {
        Ok(true)
    }

    fn map_texture<'a>(&mut self, mapping: &'a Image) -> Result<&'a [u8], io::Error> {
        Ok(&mapping.pixels)
    }
}
