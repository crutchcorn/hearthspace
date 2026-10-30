//! Copy owned pixels into validated client SHM without creating shared references.
use smithay::{
    utils::{Buffer, Size},
    wayland::{
        image_copy_capture::CaptureFailureReason as Failure,
        shm::{BufferData, with_buffer_contents_mut},
    },
};
use wayland_server::{
    Resource,
    protocol::{wl_buffer::WlBuffer, wl_shm::Format},
};

fn layout(
    data: BufferData,
    size: Size<i32, Buffer>,
    length: usize,
) -> Result<(usize, usize, usize), Failure> {
    let invalid = Failure::BufferConstraints;
    if !matches!(data.format, Format::Argb8888 | Format::Xrgb8888)
        || data.width != size.w
        || data.height != size.h
        || size.w <= 0
        || size.h <= 0
    {
        return Err(invalid);
    }
    let offset = usize::try_from(data.offset).map_err(|_| invalid)?;
    let stride = usize::try_from(data.stride).map_err(|_| invalid)?;
    let row = (size.w as usize).checked_mul(4).ok_or(invalid)?;
    let end = (size.h as usize - 1)
        .checked_mul(stride)
        .and_then(|bytes| bytes.checked_add(row))
        .and_then(|bytes| bytes.checked_add(offset))
        .ok_or(invalid)?;
    if stride < row || end > length {
        return Err(invalid);
    }
    Ok((offset, stride, row))
}

pub(super) fn copy_rgba(
    buffer: &WlBuffer,
    size: Size<i32, Buffer>,
    rgba: &[u8],
) -> Result<(), Failure> {
    if !buffer.is_alive() {
        return Err(Failure::Stopped);
    }
    with_buffer_contents_mut(buffer, |pointer, length, data| {
        let (offset, stride, row) = layout(data, size, length)?;
        if rgba.len() != row * size.h as usize {
            return Err(Failure::Unknown);
        }
        let mut converted = vec![0u8; row];
        for (y, input) in rgba.chunks_exact(row).enumerate() {
            convert_row(input, &mut converted, data.format);
            // SAFETY: layout checked the entire destination range against the
            // pool mapping. The source is owned, does not overlap it, and the
            // destination remains a raw pointer because clients may mutate SHM.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    converted.as_ptr(),
                    pointer.add(offset + y * stride),
                    row,
                );
            }
        }
        Ok(())
    })
    .map_err(|_| Failure::BufferConstraints)?
}

fn convert_row(rgba: &[u8], destination: &mut [u8], format: Format) {
    for (source, target) in rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(destination.as_chunks_mut::<4>().0.iter_mut())
    {
        let alpha = if format == Format::Xrgb8888 {
            255
        } else {
            source[3]
        };
        let pixel = (u32::from(alpha) << 24)
            | (u32::from(source[0]) << 16)
            | (u32::from(source[1]) << 8)
            | u32::from(source[2]);
        target.copy_from_slice(&pixel.to_ne_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> BufferData {
        BufferData {
            offset: 3,
            width: 2,
            height: 2,
            stride: 12,
            format: Format::Xrgb8888,
        }
    }

    #[test]
    fn validates_offsets_stride_extent_formats_and_overflow() {
        assert_eq!(layout(data(), (2, 2).into(), 23), Ok((3, 12, 8)));
        assert!(layout(data(), (2, 2).into(), 22).is_err());
        assert!(
            layout(
                BufferData {
                    stride: 7,
                    ..data()
                },
                (2, 2).into(),
                100
            )
            .is_err()
        );
        assert!(
            layout(
                BufferData {
                    offset: -1,
                    ..data()
                },
                (2, 2).into(),
                100
            )
            .is_err()
        );
        assert!(
            layout(
                BufferData {
                    format: Format::Rgb565,
                    ..data()
                },
                (2, 2).into(),
                100
            )
            .is_err()
        );
        assert!(layout(data(), (3, 2).into(), 100).is_err());
        assert!(layout(data(), (0, 2).into(), 100).is_err());
    }

    #[test]
    fn converts_colors_and_opaque_padding_in_native_endian() {
        let mut row = [0; 8];
        convert_row(&[255, 0, 0, 40, 0, 13, 255, 90], &mut row, Format::Xrgb8888);
        assert_eq!(&row[..4], &0xffff0000_u32.to_ne_bytes());
        assert_eq!(&row[4..], &0xff000dff_u32.to_ne_bytes());
        convert_row(&[255, 0, 0, 40, 0, 13, 255, 90], &mut row, Format::Argb8888);
        assert_eq!(&row[..4], &0x28ff0000_u32.to_ne_bytes());
        assert_eq!(&row[4..], &0x5a000dff_u32.to_ne_bytes());
    }
}
