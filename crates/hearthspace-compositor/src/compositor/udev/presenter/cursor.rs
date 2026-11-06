use super::super::cursor_snapshot::CursorSnapshot;
use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            element::{
                Id, Kind,
                memory::{MemoryBuffer, MemoryRenderBuffer, MemoryRenderBufferRenderElement},
            },
            gles::GlesRenderer,
        },
    },
    utils::{Buffer, Size, Transform},
};

#[derive(Default)]
pub(super) struct CursorCache {
    image: Option<(Id, u64, MemoryRenderBuffer)>,
}

impl CursorCache {
    pub fn element(
        &mut self,
        renderer: &mut GlesRenderer,
        snapshot: &CursorSnapshot,
        caps: Size<u32, Buffer>,
    ) -> Result<MemoryRenderBufferRenderElement<GlesRenderer>, String> {
        let changed = self.image.as_ref().is_none_or(|(id, revision, _)| {
            *id != snapshot.image_id || *revision != snapshot.revision
        });
        if changed {
            let memory = padded_image(&snapshot.image, caps)?;
            self.image = Some((
                snapshot.image_id.clone(),
                snapshot.revision,
                MemoryRenderBuffer::from_memory(memory, 1, Transform::Normal, None),
            ));
        }
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            snapshot.location.to_f64(),
            &self.image.as_ref().unwrap().2,
            None,
            None,
            None,
            Kind::Cursor,
        )
        .map_err(|error| error.to_string())
    }
}

// Smithay's fast upload copies the supplied image rows. Supplying the complete
// cursor extent also clears every unused pixel of an otherwise uninitialized BO.
fn padded_image(image: &MemoryBuffer, caps: Size<u32, Buffer>) -> Result<MemoryBuffer, String> {
    let size = image.size();
    if image.format() != Fourcc::Argb8888 || size.w <= 0 || size.h <= 0 {
        return Err("invalid native cursor image format or dimensions".into());
    }
    let width = i32::try_from(caps.w).unwrap_or(0);
    let height = i32::try_from(caps.h).unwrap_or(0);
    if size.w > width || size.h > height || width <= 0 || height <= 0 {
        // An oversized image remains an ordinary render element and Smithay
        // declines plane assignment. Do not crop client artwork to the plane.
        return Ok(image.clone());
    }
    let length = usize::try_from(width)
        .ok()
        .and_then(|w| w.checked_mul(height as usize))
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|bytes| *bytes <= 16 * 1024 * 1024)
        .ok_or("cursor plane extent exceeds allocation limit")?;
    let mut pixels = vec![0; length];
    let source_stride = image.stride() as usize;
    let row_bytes = size.w as usize * 4;
    if source_stride != row_bytes || image.len() < source_stride * size.h as usize {
        return Err("native cursor snapshot is not tightly packed".into());
    }
    for y in 0..size.h as usize {
        let destination = y * width as usize * 4;
        let source = y * source_stride;
        pixels[destination..destination + row_bytes]
            .copy_from_slice(&image[source..source + row_bytes]);
    }
    Ok(MemoryBuffer::from_slice(
        &pixels,
        Fourcc::Argb8888,
        (width, height),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_padding_clears_every_unused_pixel() {
        let source =
            MemoryBuffer::from_slice(&[1, 2, 3, 255, 4, 5, 6, 255], Fourcc::Argb8888, (1, 2));
        let padded = padded_image(&source, (3, 3).into()).unwrap();
        assert_eq!(&padded[..4], &[1, 2, 3, 255]);
        assert_eq!(&padded[12..16], &[4, 5, 6, 255]);
        assert!(
            padded[4..12]
                .iter()
                .chain(&padded[16..])
                .all(|byte| *byte == 0)
        );
    }

    #[test]
    fn oversized_client_cursor_is_preserved_for_composition() {
        let source = MemoryBuffer::from_slice(&[9; 8 * 4], Fourcc::Argb8888, (8, 1));
        let padded = padded_image(&source, (4, 4).into()).unwrap();
        assert_eq!(padded.size(), source.size());
        assert_eq!(&*padded, &*source);
    }
}
