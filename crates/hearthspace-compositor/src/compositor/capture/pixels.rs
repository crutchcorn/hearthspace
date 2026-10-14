use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            ExportMem,
            gles::{GlesRenderer, GlesTarget},
        },
    },
    utils::{Buffer, Physical, Rectangle, Size},
};

type CaptureResult<T> = Result<T, Box<dyn std::error::Error>>;
const MAX_PIXELS: usize = 16_777_216;

fn dimensions(size: Size<i32, Physical>) -> CaptureResult<(usize, usize)> {
    let width = usize::try_from(size.w)?;
    let height = usize::try_from(size.h)?;
    let count = width
        .checked_mul(height)
        .ok_or("capture dimensions overflow")?;
    if count == 0 || count > MAX_PIXELS {
        return Err("capture exceeds 16 megapixel limit or has zero size".into());
    }
    Ok((width, height))
}

pub(in crate::compositor) fn read_pixels(
    renderer: &mut GlesRenderer,
    framebuffer: &GlesTarget<'_>,
    size: Size<i32, Physical>,
    bottom_up: bool,
) -> CaptureResult<Vec<u8>> {
    let (width, height) = dimensions(size)?;
    let region = Rectangle::from_size(Size::<i32, Buffer>::from((size.w, size.h)));
    let mapping = renderer.copy_framebuffer(framebuffer, region, Fourcc::Abgr8888)?;
    let pixels = renderer.map_texture(&mapping)?;
    if pixels.len() != width * height * 4 {
        return Err("capture readback length mismatch".into());
    }
    if bottom_up {
        Ok(pixels
            .chunks_exact(width * 4)
            .rev()
            .flatten()
            .copied()
            .collect())
    } else {
        Ok(pixels.to_vec())
    }
}

pub(in crate::compositor) fn encode_png(
    size: Size<i32, Physical>,
    pixels: &[u8],
) -> CaptureResult<Vec<u8>> {
    let (width, height) = dimensions(size)?;
    if pixels.len() != width * height * 4 {
        return Err("capture pixel length mismatch".into());
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        // Readback runs on the compositor thread; prioritize interactive latency.
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(pixels)?;
    }
    Ok(bytes)
}

#[cfg(feature = "udev")]
pub(in crate::compositor) struct CursorOverlay {
    pub image: super::super::cursor::CursorImage,
    pub plane_size: (u32, u32),
    pub x: i32,
    pub y: i32,
}

#[cfg(feature = "udev")]
impl CursorOverlay {
    // Xcursor pixels are premultiplied RGBA, matching the DRM plane blend mode.
    pub(in crate::compositor) fn paint(&self, pixels: &mut [u8], size: Size<i32, Physical>) {
        for row in 0..self.image.height.min(self.plane_size.1) as i32 {
            let y = self.y + row;
            if y < 0 || y >= size.h {
                continue;
            }
            for column in 0..self.image.width.min(self.plane_size.0) as i32 {
                let x = self.x + column;
                if x < 0 || x >= size.w {
                    continue;
                }
                let src = ((row as u32 * self.image.width + column as u32) * 4) as usize;
                let dst = ((y * size.w + x) * 4) as usize;
                let alpha = u16::from(self.image.pixels_rgba[src + 3]);
                for channel in 0..3 {
                    pixels[dst + channel] = (u16::from(self.image.pixels_rgba[src + channel])
                        + (u16::from(pixels[dst + channel]) * (255 - alpha) + 127) / 255)
                        .min(255) as u8;
                }
                pixels[dst + 3] =
                    (alpha + (u16::from(pixels[dst + 3]) * (255 - alpha) + 127) / 255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_dimensions_and_pixel_lengths() {
        for size in [(0, 2), (i32::MAX, i32::MAX), (4097, 4096)] {
            assert!(encode_png(size.into(), &[]).is_err());
        }
        assert!(encode_png((2, 2).into(), &[0; 12]).is_err());
    }

    #[test]
    fn png_preserves_top_down_pixel_orientation() {
        let pixels = [255, 0, 0, 255, 0, 0, 255, 255];
        let png = encode_png((1, 2).into(), &pixels).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        let mut result = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut result).unwrap();
        assert_eq!(result, pixels);
    }

    #[cfg(feature = "udev")]
    #[test]
    fn hardware_cursor_clips_and_blends_once() {
        let overlay = CursorOverlay {
            plane_size: (2, 1),
            x: -1,
            y: 0,
            image: super::super::super::cursor::CursorImage {
                width: 2,
                height: 1,
                hotspot: (0, 0),
                pixels_rgba: vec![255, 0, 0, 255, 0, 128, 0, 128],
            },
        };
        let mut pixels = [0, 0, 255, 255, 255, 255, 255, 255];
        overlay.paint(&mut pixels, (2, 1).into());
        assert_eq!(pixels, [0, 128, 127, 255, 255, 255, 255, 255]);
    }

    #[cfg(feature = "udev")]
    #[test]
    fn hardware_cursor_respects_plane_size_and_source_stride() {
        let overlay = CursorOverlay {
            x: 0,
            y: 0,
            plane_size: (1, 2),
            image: super::super::super::cursor::CursorImage {
                width: 2,
                height: 2,
                hotspot: (0, 0),
                pixels_rgba: vec![
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 255, 0, 255,
                ],
            },
        };
        let mut pixels = [0; 16];
        overlay.paint(&mut pixels, (2, 2).into());
        assert_eq!(
            pixels,
            [255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0]
        );
    }
}
