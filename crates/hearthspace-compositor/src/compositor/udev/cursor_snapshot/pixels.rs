use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryBuffer},
    utils::{Physical, Point, Size},
    wayland::shm::{BufferAccessError, with_buffer_contents},
};
use wayland_server::protocol::{wl_buffer::WlBuffer, wl_shm::Format};

use super::SnapshotResult;
use crate::compositor::cursor::CursorImage;

// Match capture's bound, independently of cursor-plane capabilities. Oversized
// hardware cursors remain eligible for the presenter's software fallback.
const MAX_PIXELS: usize = 16_777_216;

pub(super) fn dimensions(size: Size<i32, Physical>) -> SnapshotResult<(usize, usize)> {
    let width = usize::try_from(size.w)?;
    let height = usize::try_from(size.h)?;
    let count = width
        .checked_mul(height)
        .ok_or("cursor dimensions overflow")?;
    if count == 0 || count > MAX_PIXELS {
        return Err("cursor exceeds 16 megapixel limit or has zero size".into());
    }
    Ok((width, height))
}

pub(super) fn position(
    pointer: Point<f64, Physical>,
    hotspot: Point<f64, Physical>,
    offset: Point<i32, Physical>,
) -> SnapshotResult<Point<i32, Physical>> {
    let coordinate = |pointer: f64, hotspot: f64, offset: i32| -> SnapshotResult<i32> {
        let value = pointer.round() - hotspot + f64::from(offset);
        if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
            return Err("cursor location exceeds physical coordinate range".into());
        }
        Ok(value as i32)
    };
    Ok((
        coordinate(pointer.x, hotspot.x, offset.x)?,
        coordinate(pointer.y, hotspot.y, offset.y)?,
    )
        .into())
}

pub(super) fn rgba(mut pixels: Vec<u8>, size: Size<i32, Physical>) -> SnapshotResult<MemoryBuffer> {
    let (width, height) = dimensions(size)?;
    if pixels.len() != width * height * 4 {
        return Err("cursor pixel length mismatch".into());
    }
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let argb = (u32::from(pixel[3]) << 24)
            | (u32::from(pixel[0]) << 16)
            | (u32::from(pixel[1]) << 8)
            | u32::from(pixel[2]);
        pixel.copy_from_slice(&argb.to_ne_bytes());
    }
    Ok(MemoryBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (size.w, size.h),
    ))
}

pub(super) fn named(
    source: CursorImage,
    scale: i32,
) -> SnapshotResult<(MemoryBuffer, Point<i32, Physical>)> {
    let width = i32::try_from(source.width)?
        .checked_mul(scale)
        .ok_or("cursor width overflow")?;
    let height = i32::try_from(source.height)?
        .checked_mul(scale)
        .ok_or("cursor height overflow")?;
    let size = Size::from((width, height));
    dimensions(size)?;
    let hotspot_x = source
        .hotspot
        .0
        .checked_mul(scale)
        .and_then(i32::checked_neg)
        .ok_or("cursor hotspot overflow")?;
    let hotspot_y = source
        .hotspot
        .1
        .checked_mul(scale)
        .and_then(i32::checked_neg)
        .ok_or("cursor hotspot overflow")?;
    let pixels = if scale == 1 {
        source.pixels_rgba
    } else {
        let image = image::RgbaImage::from_raw(source.width, source.height, source.pixels_rgba)
            .ok_or("invalid packaged cursor image")?;
        image::imageops::resize(
            &image,
            width as u32,
            height as u32,
            image::imageops::FilterType::Triangle,
        )
        .into_raw()
    };
    Ok((rgba(pixels, size)?, (hotspot_x, hotspot_y).into()))
}

/// Copy a simple, untransformed SHM image while Smithay owns the pool mapping.
/// Unsupported storage/format takes the renderer path; bad mappings are errors.
pub(super) fn shm(
    buffer: &WlBuffer,
    size: Size<i32, Physical>,
) -> SnapshotResult<Option<MemoryBuffer>> {
    let (width, height) = dimensions(size)?;
    match with_buffer_contents(buffer, |ptr, len, data| -> SnapshotResult<_> {
        if data.width != size.w || data.height != size.h {
            return Ok(None);
        }
        if !matches!(
            data.format,
            Format::Argb8888 | Format::Xrgb8888 | Format::Abgr8888 | Format::Xbgr8888
        ) {
            return Ok(None);
        }
        let offset = usize::try_from(data.offset)?;
        let stride = usize::try_from(data.stride)?;
        let row_bytes = width * 4;
        let end = (height - 1)
            .checked_mul(stride)
            .and_then(|n| n.checked_add(offset))
            .and_then(|n| n.checked_add(row_bytes))
            .ok_or("cursor SHM range overflow")?;
        if stride < row_bytes || end > len {
            return Err("cursor SHM buffer exceeds pool bounds".into());
        }
        let mut pixels = vec![0; row_bytes * height];
        for row in 0..height {
            // SAFETY: the checked range is in Smithay's live pool mapping. Copy
            // raw bytes without making a Rust reference into client-owned SHM.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    ptr.add(offset + row * stride),
                    pixels.as_mut_ptr().add(row * row_bytes),
                    row_bytes,
                );
            }
        }
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let mut value = u32::from_ne_bytes(*pixel);
            if matches!(data.format, Format::Abgr8888 | Format::Xbgr8888) {
                value = (value & 0xff00_ff00) | ((value & 0xff) << 16) | ((value >> 16) & 0xff);
            }
            if matches!(data.format, Format::Xrgb8888 | Format::Xbgr8888) {
                value |= 0xff00_0000;
            }
            pixel.copy_from_slice(&value.to_ne_bytes());
        }
        Ok(Some(MemoryBuffer::from_slice(
            &pixels,
            Fourcc::Argb8888,
            (size.w, size.h),
        )))
    }) {
        Ok(result) => result,
        Err(BufferAccessError::NotManaged) => Ok(None),
        Err(error) => Err(error.into()),
    }
}
