use super::BridgeError;
use ash::vk;
use smithay::{
    backend::allocator::{Buffer, Format, Fourcc, Modifier, dmabuf::Dmabuf},
    reexports::rustix::fs::{SeekFrom, seek},
};

pub(crate) const FORMATS: [Fourcc; 4] = [
    Fourcc::Abgr8888,
    Fourcc::Argb8888,
    Fourcc::Xbgr8888,
    Fourcc::Xrgb8888,
];

pub(super) fn vk_format(code: Fourcc) -> Result<vk::Format, BridgeError> {
    if !cfg!(target_endian = "little") {
        return Err(BridgeError::Unsupported(
            "8888 byte mapping requires little endian",
        ));
    }
    match code {
        Fourcc::Abgr8888 | Fourcc::Xbgr8888 => Ok(vk::Format::R8G8B8A8_UNORM),
        Fourcc::Argb8888 | Fourcc::Xrgb8888 => Ok(vk::Format::B8G8R8A8_UNORM),
        _ => Err(BridgeError::Unsupported(
            "only four packed 8888 formats are supported",
        )),
    }
}

/// Actual exported metadata, never inferred from a requested modifier or width.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BufferLayout {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub offset: u64,
    pub stride: u32,
    pub fd_size: u64,
}

impl BufferLayout {
    pub(super) fn inspect(dma: &Dmabuf) -> Result<Self, BridgeError> {
        vk_format(dma.format().code)?;
        if dma.size().w <= 0 || dma.size().h <= 0 || dma.num_planes() != 1 {
            return Err(BridgeError::Invalid(
                "positive dimensions and one memory plane required",
            ));
        }
        if dma.format().modifier == Modifier::Invalid || !dma.flags().is_empty() {
            return Err(BridgeError::Unsupported(
                "explicit modifier and uninverted storage required",
            ));
        }
        let fd = dma
            .handles()
            .next()
            .ok_or(BridgeError::Invalid("missing DMA-BUF fd"))?;
        let fd_size = seek(fd, SeekFrom::End(0)).map_err(|e| BridgeError::io("DMA-BUF size", e))?;
        seek(fd, SeekFrom::Start(0)).map_err(|e| BridgeError::io("reset DMA-BUF offset", e))?;
        let layout = Self {
            format: dma.format(),
            width: dma.width(),
            height: dma.height(),
            offset: u64::from(
                dma.offsets()
                    .next()
                    .ok_or(BridgeError::Invalid("missing offset"))?,
            ),
            stride: dma
                .strides()
                .next()
                .ok_or(BridgeError::Invalid("missing stride"))?,
            fd_size,
        };
        if layout.fd_size == 0 || layout.offset >= layout.fd_size {
            return Err(BridgeError::Invalid(
                "plane offset outside DMA-BUF allocation",
            ));
        }
        Ok(layout)
    }

    pub(super) fn linear_extent(self) -> Result<u64, BridgeError> {
        if self.format.modifier != Modifier::Linear {
            return Err(BridgeError::Unsupported(
                "destination must be explicitly Linear",
            ));
        }
        let row = u64::from(self.width) * 4;
        if self.width == 0
            || self.height == 0
            || u64::from(self.stride) < row
            || !self.stride.is_multiple_of(4)
            || !self.offset.is_multiple_of(4)
        {
            return Err(BridgeError::Invalid("invalid packed-8888 row layout"));
        }
        let end = self
            .offset
            .checked_add(u64::from(self.height - 1) * u64::from(self.stride))
            .and_then(|n| n.checked_add(row))
            .ok_or(BridgeError::Invalid("Linear extent overflow"))?;
        if end > self.fd_size {
            return Err(BridgeError::Invalid(
                "Linear extent exceeds DMA-BUF allocation",
            ));
        }
        Ok(end)
    }

    pub(super) fn region(self) -> vk::BufferImageCopy {
        vk::BufferImageCopy::default()
            .buffer_offset(self.offset)
            .buffer_row_length(self.stride / 4)
            .buffer_image_height(self.height)
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D {
                width: self.width,
                height: self.height,
                depth: 1,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padded() -> BufferLayout {
        BufferLayout {
            format: Format {
                code: Fourcc::Abgr8888,
                modifier: Modifier::Linear,
            },
            width: 3,
            height: 2,
            offset: 16,
            stride: 32,
            fd_size: 60,
        }
    }

    #[test]
    fn padded_copy_includes_offset_once_and_excludes_last_row_padding() {
        assert_eq!(padded().linear_extent().unwrap(), 60);
        let region = padded().region();
        assert_eq!(region.buffer_offset, 16);
        assert_eq!(region.buffer_row_length, 8);
        assert!(
            BufferLayout {
                fd_size: 59,
                ..padded()
            }
            .linear_extent()
            .is_err()
        );
    }

    #[test]
    fn rejects_unrepresentable_rows_and_overflowing_plane_extent() {
        assert!(
            BufferLayout {
                stride: 13,
                ..padded()
            }
            .linear_extent()
            .is_err()
        );
        assert!(
            BufferLayout {
                offset: u64::MAX - 3,
                fd_size: u64::MAX,
                ..padded()
            }
            .linear_extent()
            .is_err()
        );
        assert!(
            BufferLayout {
                height: 0,
                ..padded()
            }
            .linear_extent()
            .is_err()
        );
    }
}
