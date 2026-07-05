use std::{fs, io::Cursor};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            ContextId, ImportMem, Renderer, RendererSuper,
            element::{Id, Kind, texture::TextureRenderElement},
            gles::GlesRenderer,
        },
    },
    utils::{Logical, Physical, Point, Rectangle, Size, Transform},
};
use tracing::warn;

use crate::geometry::CanvasPoint;

use super::GlesTexture;
use crate::compositor::App;

const MAX_BACKGROUND_TILE_ELEMENTS: usize = 512;
const DEFAULT_BACKGROUND_IMAGE_PNG: &[u8] =
    include_bytes!("../../../../../assets/backgrounds/default.png");

pub(in crate::compositor) struct BackgroundImageTile {
    context_id: ContextId<GlesTexture>,
    texture: GlesTexture,
    size: Size<i32, Logical>,
}

struct BackgroundImageData {
    pixels: Vec<u8>,
    format: Fourcc,
    size: Size<i32, Logical>,
}

impl App {
    /// Build repeated background image render elements for the current viewport.
    ///
    /// The tile is laid out in *canvas* space, so it pans and zooms with the
    /// viewport by default while keeping the render-element count proportional
    /// to visible image tiles, not to details inside the image. This is also the
    /// shape future user-defined repeating backgrounds can reuse.
    pub(super) fn background_image_elements(
        &mut self,
        renderer: &mut GlesRenderer,
        output_rect: Rectangle<i32, Logical>,
    ) -> Vec<TextureRenderElement<GlesTexture>> {
        if self.viewport_scale <= 0.0 || !self.viewport_scale.is_finite() {
            return Vec::new();
        }
        let moves_with_canvas = self.background_moves_with_canvas;

        let (context_id, texture, tile_size) = match self.background_image_tile(renderer) {
            Ok(tile) => (tile.context_id.clone(), tile.texture.clone(), tile.size),
            Err(_) => return Vec::new(),
        };

        let (min_x, max_x, min_y, max_y) = background_image_visible_bounds(
            output_rect,
            moves_with_canvas,
            self.viewport_offset,
            self.viewport_scale,
        );

        let Some((first_x, last_x, x_count)) =
            background_tile_axis_bounds(min_x, max_x, f64::from(tile_size.w))
        else {
            return Vec::new();
        };
        let Some((first_y, last_y, y_count)) =
            background_tile_axis_bounds(min_y, max_y, f64::from(tile_size.h))
        else {
            return Vec::new();
        };
        let tile_count = x_count.saturating_mul(y_count);
        if tile_count > MAX_BACKGROUND_TILE_ELEMENTS {
            return Vec::new();
        }

        while self.background_tile_ids.len() < tile_count {
            self.background_tile_ids.push(Id::new());
        }

        let mut elements = Vec::with_capacity(tile_count);
        let src = Rectangle::from_size(tile_size.to_f64());
        for gy in first_y..=last_y {
            for gx in first_x..=last_x {
                let tile_origin = Point::from((
                    gx as f64 * f64::from(tile_size.w),
                    gy as f64 * f64::from(tile_size.h),
                ));
                let next_tile_origin = Point::from((
                    (gx + 1) as f64 * f64::from(tile_size.w),
                    (gy + 1) as f64 * f64::from(tile_size.h),
                ));
                let (screen, next_screen) = if moves_with_canvas {
                    (
                        self.canvas_to_screen(tile_origin),
                        self.canvas_to_screen(next_tile_origin),
                    )
                } else {
                    (tile_origin, next_tile_origin)
                };
                let left = screen.x.round() as i32;
                let top = screen.y.round() as i32;
                let right = next_screen.x.round() as i32;
                let bottom = next_screen.y.round() as i32;
                let location = Point::<f64, Physical>::from((f64::from(left), f64::from(top)));
                let size = Size::from(((right - left).max(1), (bottom - top).max(1)));
                let slot = elements.len();
                elements.push(TextureRenderElement::from_static_texture(
                    self.background_tile_ids[slot].clone(),
                    context_id.clone(),
                    location,
                    texture.clone(),
                    1,
                    Transform::Normal,
                    None,
                    Some(src),
                    Some(size),
                    None,
                    Kind::Unspecified,
                ));
            }
        }
        elements
    }

    fn background_image_tile(
        &mut self,
        renderer: &mut GlesRenderer,
    ) -> Result<&BackgroundImageTile, <GlesRenderer as RendererSuper>::Error> {
        let context_id = renderer.context_id();
        let needs_rebuild = self
            .background_image_tile
            .as_ref()
            .map(|tile| tile.context_id != context_id)
            .unwrap_or(true);

        if needs_rebuild {
            let data = self.background_image_data();
            let texture = renderer.import_memory(
                &data.pixels,
                data.format,
                (data.size.w, data.size.h).into(),
                false,
            )?;
            self.background_image_tile = Some(BackgroundImageTile {
                context_id,
                texture,
                size: data.size,
            });
        }

        Ok(self.background_image_tile.as_ref().unwrap())
    }

    fn background_image_data(&self) -> BackgroundImageData {
        if let Some(path) = &self.background_image_path {
            match fs::read(path) {
                Ok(contents) => match decode_background_png(&contents) {
                    Ok(image) => return image,
                    Err(error) => {
                        warn!(path, %error, "failed to decode background image; using default")
                    }
                },
                Err(error) => warn!(path, %error, "failed to read background image; using default"),
            }
        }

        default_background_image_data()
    }
}

fn background_tile_axis_bounds(min: f64, max: f64, tile_size: f64) -> Option<(i64, i64, usize)> {
    if !min.is_finite() || !max.is_finite() || tile_size <= 0.0 || !tile_size.is_finite() {
        return None;
    }

    let first = (min / tile_size).floor() as i64;
    let last = (max / tile_size).ceil() as i64 - 1;
    let count = usize::try_from(last.checked_sub(first)?.checked_add(1)?).ok()?;
    Some((first, last, count))
}

fn background_image_visible_bounds(
    output_rect: Rectangle<i32, Logical>,
    moves_with_canvas: bool,
    viewport_offset: CanvasPoint,
    viewport_scale: f64,
) -> (f64, f64, f64, f64) {
    let top_left = Point::from((f64::from(output_rect.loc.x), f64::from(output_rect.loc.y)));
    let bottom_right = Point::from((
        f64::from(output_rect.loc.x) + f64::from(output_rect.size.w),
        f64::from(output_rect.loc.y) + f64::from(output_rect.size.h),
    ));

    let (top_left, bottom_right) = if moves_with_canvas {
        (
            crate::geometry::screen_to_canvas(top_left, viewport_offset, viewport_scale),
            crate::geometry::screen_to_canvas(bottom_right, viewport_offset, viewport_scale),
        )
    } else {
        (top_left, bottom_right)
    };

    (
        top_left.x.min(bottom_right.x),
        top_left.x.max(bottom_right.x),
        top_left.y.min(bottom_right.y),
        top_left.y.max(bottom_right.y),
    )
}

fn default_background_image_data() -> BackgroundImageData {
    decode_background_png(DEFAULT_BACKGROUND_IMAGE_PNG)
        .expect("packaged default background image must be a valid 8-bit RGBA PNG")
}

fn decode_background_png(contents: &[u8]) -> Result<BackgroundImageData, String> {
    let mut decoder = png::Decoder::new(Cursor::new(contents));
    decoder.set_transformations(png::Transformations::ALPHA | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let mut pixels = vec![0; reader.output_buffer_size().unwrap_or_default()];
    let output = reader
        .next_frame(&mut pixels)
        .map_err(|error| error.to_string())?;
    pixels.truncate(output.buffer_size());

    if output.color_type != png::ColorType::Rgba || output.bit_depth != png::BitDepth::Eight {
        return Err(format!(
            "expected 8-bit RGBA PNG, got {:?} {:?}",
            output.bit_depth, output.color_type
        ));
    }

    Ok(BackgroundImageData {
        pixels,
        format: Fourcc::Abgr8888,
        size: Size::<i32, Logical>::from((output.width as i32, output.height as i32)),
    })
}

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Rectangle, Size};

    use crate::geometry::CanvasPoint;

    use super::{background_tile_axis_bounds, default_background_image_data};

    #[test]
    fn background_tile_axis_bounds_include_partially_visible_tiles() {
        assert_eq!(
            background_tile_axis_bounds(0.0, 240.0, 120.0),
            Some((0, 1, 2))
        );
        assert_eq!(
            background_tile_axis_bounds(-240.0, 0.0, 120.0),
            Some((-2, -1, 2))
        );
        assert_eq!(
            background_tile_axis_bounds(10.0, 240.1, 120.0),
            Some((0, 2, 3))
        );
    }

    #[test]
    fn fixed_background_bounds_ignore_viewport_pan_and_zoom() {
        let output = Rectangle::<i32, Logical>::new((0, 0).into(), (800, 600).into());

        assert_eq!(
            super::background_image_visible_bounds(output, false, CanvasPoint { x: 0, y: 0 }, 1.0,),
            super::background_image_visible_bounds(
                output,
                false,
                CanvasPoint {
                    x: 10_000,
                    y: -5_000
                },
                0.25,
            )
        );
    }

    #[test]
    fn canvas_background_bounds_follow_viewport_pan_and_zoom() {
        let output = Rectangle::<i32, Logical>::new((0, 0).into(), (800, 600).into());

        assert_ne!(
            super::background_image_visible_bounds(output, true, CanvasPoint { x: 0, y: 0 }, 1.0,),
            super::background_image_visible_bounds(
                output,
                true,
                CanvasPoint {
                    x: 10_000,
                    y: -5_000
                },
                0.25,
            )
        );
    }

    #[test]
    fn default_background_image_loads_from_png_asset() {
        let image = default_background_image_data();
        assert_eq!(image.size, Size::<i32, Logical>::from((1920, 1080)));

        assert_eq!(
            pixel_rgba(&image.pixels, image.size, 0, 0),
            [199, 199, 204, 255]
        );
        assert_eq!(
            pixel_rgba(&image.pixels, image.size, 1919, 0),
            [199, 199, 204, 255]
        );
        assert_eq!(
            pixel_rgba(&image.pixels, image.size, 60, 60),
            [255, 255, 255, 255]
        );
    }

    fn pixel_rgba(pixels: &[u8], size: Size<i32, Logical>, x: i32, y: i32) -> [u8; 4] {
        let offset = ((y * size.w + x) * 4) as usize;
        pixels[offset..offset + 4].try_into().unwrap()
    }
}
