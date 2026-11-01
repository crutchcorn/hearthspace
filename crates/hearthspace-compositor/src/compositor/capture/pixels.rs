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

pub(super) fn dimensions(size: Size<i32, Physical>) -> CaptureResult<(usize, usize)> {
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
        // Avoid adaptive row-filter evaluation: the fastest preset remains
        // lossless and trades a larger PNG for shorter agent round trips.
        encoder.set_compression(png::Compression::Fastest);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(pixels)?;
    }
    Ok(bytes)
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

    #[test]
    fn fast_png_preserves_mixed_rgba_rows_exactly() {
        // Odd dimensions exercise row boundaries. Include gradients, noise,
        // repeated rows, transparency and sharp edges rather than only solids.
        let (width, height) = (513, 257);
        let mut pixels = Vec::with_capacity(width * height * 4);
        let mut noise = 0x6d2b_79f5_u32;
        for y in 0..height {
            for x in 0..width {
                noise ^= noise << 13;
                noise ^= noise >> 17;
                noise ^= noise << 5;
                let rgba = match (y / 17) % 4 {
                    0 => [x as u8, (x / 2) as u8, (x / 8) as u8, (x % 256) as u8],
                    1 => noise.to_le_bytes(),
                    2 if (x / 7 + y / 5) % 2 == 0 => [255, 0, 173, 0],
                    2 => [0, 255, 41, 255],
                    _ => [17, 83, 191, 200],
                };
                pixels.extend_from_slice(&rgba);
            }
        }
        let png = encode_png((width as i32, height as i32).into(), &pixels).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        let info = reader.info();
        assert_eq!((info.width, info.height), (width as u32, height as u32));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
        let frame = reader.next_frame(&mut decoded).unwrap();
        assert_eq!(&decoded[..frame.buffer_size()], pixels.as_slice());
    }
}
