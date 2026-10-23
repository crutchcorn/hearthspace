//! The experiment's quiet dotted field, generated once per renderer texture.
//! Tile dimensions are whole grid periods, so panning never reveals a seam.
use super::{BackgroundImageData, Fourcc};
use crate::geometry::grid::{CANVAS_GRID_OFFSET, CANVAS_GRID_SIZE};

pub(super) fn image(mode: hearthspace_ipc::ColorMode) -> BackgroundImageData {
    let (background, dot) = match mode {
        hearthspace_ipc::ColorMode::Light => ([229, 233, 227, 255], [208, 215, 209, 255]),
        hearthspace_ipc::ColorMode::Dark => ([20, 34, 37, 255], [41, 57, 58, 255]),
    };
    let step = CANVAS_GRID_SIZE as usize;
    let offset = CANVAS_GRID_OFFSET as usize;
    let (width, height) = (step * 64, step * 36);
    let mut pixels = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(if x % step == offset && y % step == offset {
                &dot
            } else {
                &background
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

#[cfg(test)]
mod tests {
    #[test]
    fn both_modes_keep_the_same_grid_geometry_with_visible_dots() {
        use hearthspace_ipc::ColorMode;
        let light = super::image(ColorMode::Light);
        let dark = super::image(ColorMode::Dark);
        assert_eq!(light.size, dark.size);
        let dot = ((14 * dark.size.w + 14) * 4) as usize;
        assert!(dark.pixels[0] < light.pixels[0]);
        assert!(dark.pixels[dot] > dark.pixels[0]);
        assert!(light.pixels[dot] < light.pixels[0]);
        assert!(
            dark.pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[3] == 255)
        );
    }
}
