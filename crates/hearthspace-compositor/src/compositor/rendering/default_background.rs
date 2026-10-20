//! The experiment's quiet dotted field, generated once per renderer texture.
//! Tile dimensions are whole grid periods, so panning never reveals a seam.
use super::{BackgroundImageData, Fourcc};

pub(super) fn image() -> BackgroundImageData {
    let (width, height) = (28 * 64, 28 * 36);
    let mut pixels = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(if x % 28 == 14 && y % 28 == 14 {
                &[208, 215, 209, 255]
            } else {
                &[229, 233, 227, 255]
            });
        }
    }
    BackgroundImageData {
        pixels,
        format: Fourcc::Abgr8888,
        size: (width as i32, height as i32).into(),
        logical_grid: true,
    }
}
