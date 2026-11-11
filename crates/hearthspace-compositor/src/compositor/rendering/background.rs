use std::{fs, io::Cursor};

use hearthspace_ipc::BackgroundMode;
use image::ImageDecoder as _;
use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            ContextId, ImportMem, Renderer,
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

pub(in crate::compositor) struct BackgroundImageTile {
    context_id: ContextId<GlesTexture>,
    texture: Option<GlesTexture>,
    size: Size<i32, Logical>,
}

struct BackgroundImageData {
    pixels: Vec<u8>,
    format: Fourcc,
    size: Size<i32, Logical>,
}

impl App {
    /// Theme changes damage shader dots, while user photos keep their texture.
    pub(in crate::compositor) fn invalidate_default_background(&mut self) -> bool {
        self.background_image_path.is_none()
    }

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
        output_scale: i32,
    ) -> Vec<TextureRenderElement<GlesTexture>> {
        if self.background_image_path.is_none()
            || self.viewport_scale <= 0.0
            || !self.viewport_scale.is_finite()
        {
            return Vec::new();
        }
        let (context_id, texture, tile_size) = match self.background_image_tile(renderer) {
            Ok(tile) => match tile.texture.clone() {
                Some(texture) => (tile.context_id.clone(), texture, tile.size),
                None => return Vec::new(),
            },
            Err(_) => return Vec::new(),
        };

        let output_rect = wallpaper_presentation_rect(output_rect, output_scale);
        if self.background_mode != BackgroundMode::RepeatWithCanvas {
            return self.fixed_background_image_element(
                context_id,
                texture,
                tile_size,
                output_rect,
                self.background_mode,
            );
        }

        let (min_x, max_x, min_y, max_y) =
            background_image_visible_bounds(output_rect, self.viewport_offset, self.viewport_scale);

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
                let tile_origin = Point::<f64, Logical>::from((
                    gx as f64 * f64::from(tile_size.w),
                    gy as f64 * f64::from(tile_size.h),
                ));
                let next_tile_origin = Point::<f64, Logical>::from((
                    (gx + 1) as f64 * f64::from(tile_size.w),
                    (gy + 1) as f64 * f64::from(tile_size.h),
                ));
                let screen = self.canvas_to_screen(tile_origin);
                let next_screen = self.canvas_to_screen(next_tile_origin);
                let screen: Point<i32, Logical> = screen.to_i32_round();
                let next_screen: Point<i32, Logical> = next_screen.to_i32_round();
                let left = screen.x;
                let top = screen.y;
                let right = next_screen.x;
                let bottom = next_screen.y;
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

    fn fixed_background_image_element(
        &mut self,
        context_id: ContextId<GlesTexture>,
        texture: GlesTexture,
        image_size: Size<i32, Logical>,
        output_rect: Rectangle<i32, Logical>,
        mode: BackgroundMode,
    ) -> Vec<TextureRenderElement<GlesTexture>> {
        let Some((src, dst)) = fixed_background_rects(image_size, output_rect, mode) else {
            return Vec::new();
        };

        if self.background_tile_ids.is_empty() {
            self.background_tile_ids.push(Id::new());
        }

        vec![TextureRenderElement::from_static_texture(
            self.background_tile_ids[0].clone(),
            context_id,
            dst.loc.to_physical(1).to_f64(),
            texture,
            1,
            Transform::Normal,
            None,
            Some(src),
            Some(dst.size),
            None,
            Kind::Unspecified,
        )]
    }

    fn background_image_tile(
        &mut self,
        renderer: &mut GlesRenderer,
    ) -> Result<&BackgroundImageTile, Box<dyn std::error::Error>> {
        let context_id = renderer.context_id();
        let needs_rebuild = self
            .background_image_tile
            .as_ref()
            .map(|tile| tile.context_id != context_id)
            .unwrap_or(true);

        if needs_rebuild {
            let data = match self.background_image_data() {
                Ok(data) => data,
                Err(_) => {
                    self.background_image_tile = Some(BackgroundImageTile {
                        context_id,
                        texture: None,
                        size: (0, 0).into(),
                    });
                    return Ok(self.background_image_tile.as_ref().unwrap());
                }
            };
            let texture = renderer.import_memory(
                &data.pixels,
                data.format,
                (data.size.w, data.size.h).into(),
                false,
            )?;
            self.background_image_tile = Some(BackgroundImageTile {
                context_id,
                texture: Some(texture),
                size: data.size,
            });
        }

        Ok(self.background_image_tile.as_ref().unwrap())
    }

    fn background_image_data(&self) -> Result<BackgroundImageData, String> {
        let path = self
            .background_image_path
            .as_ref()
            .ok_or("No photo selected")?;
        let result = fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|contents| decode_background_image(&contents));
        if let Err(error) = &result {
            warn!(path, %error, "failed to load background image; using GLSL dots");
        }
        result
    }
}

fn wallpaper_presentation_rect(
    output_rect: Rectangle<i32, Logical>,
    output_scale: i32,
) -> Rectangle<i32, Logical> {
    let scale = output_scale.max(1);
    Rectangle::new(
        output_rect.loc,
        Size::from((output_rect.size.w * scale, output_rect.size.h * scale)),
    )
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
    viewport_offset: CanvasPoint,
    viewport_scale: f64,
) -> (f64, f64, f64, f64) {
    let top_left = Point::from((f64::from(output_rect.loc.x), f64::from(output_rect.loc.y)));
    let bottom_right = Point::from((
        f64::from(output_rect.loc.x) + f64::from(output_rect.size.w),
        f64::from(output_rect.loc.y) + f64::from(output_rect.size.h),
    ));

    let top_left = crate::geometry::screen_to_canvas(top_left, viewport_offset, viewport_scale);
    let bottom_right =
        crate::geometry::screen_to_canvas(bottom_right, viewport_offset, viewport_scale);

    (
        top_left.x.min(bottom_right.x),
        top_left.x.max(bottom_right.x),
        top_left.y.min(bottom_right.y),
        top_left.y.max(bottom_right.y),
    )
}

fn fixed_background_rects(
    image_size: Size<i32, Logical>,
    output_rect: Rectangle<i32, Logical>,
    mode: BackgroundMode,
) -> Option<(Rectangle<f64, Logical>, Rectangle<i32, Logical>)> {
    if image_size.w <= 0 || image_size.h <= 0 || output_rect.size.w <= 0 || output_rect.size.h <= 0
    {
        return None;
    }

    let image_w = f64::from(image_size.w);
    let image_h = f64::from(image_size.h);
    let output_w = f64::from(output_rect.size.w);
    let output_h = f64::from(output_rect.size.h);
    let full_src = Rectangle::from_size(Size::<f64, Logical>::from((image_w, image_h)));

    match mode {
        BackgroundMode::RepeatWithCanvas => None,
        BackgroundMode::Stretch => Some((full_src, output_rect)),
        BackgroundMode::Contains => {
            let scale = (output_w / image_w).min(output_h / image_h);
            let dst_w = (image_w * scale).round().max(1.0) as i32;
            let dst_h = (image_h * scale).round().max(1.0) as i32;
            let dst = Rectangle::new(
                (
                    output_rect.loc.x + (output_rect.size.w - dst_w) / 2,
                    output_rect.loc.y + (output_rect.size.h - dst_h) / 2,
                )
                    .into(),
                (dst_w, dst_h).into(),
            );
            Some((full_src, dst))
        }
        BackgroundMode::Cover => {
            let output_aspect = output_w / output_h;
            let image_aspect = image_w / image_h;
            let src = if image_aspect > output_aspect {
                let src_w = image_h * output_aspect;
                Rectangle::new(
                    ((image_w - src_w) / 2.0, 0.0).into(),
                    (src_w, image_h).into(),
                )
            } else {
                let src_h = image_w / output_aspect;
                Rectangle::new(
                    (0.0, (image_h - src_h) / 2.0).into(),
                    (image_w, src_h).into(),
                )
            };
            Some((src, output_rect))
        }
    }
}

fn decode_background_image(contents: &[u8]) -> Result<BackgroundImageData, String> {
    match image::guess_format(contents) {
        Ok(image::ImageFormat::Png) => decode_background_png(contents),
        Ok(image::ImageFormat::Jpeg) => decode_background_jpeg(contents),
        _ => Err("expected a PNG or JPEG wallpaper".into()),
    }
}

fn decode_background_jpeg(contents: &[u8]) -> Result<BackgroundImageData, String> {
    let mut reader =
        image::ImageReader::with_format(Cursor::new(contents), image::ImageFormat::Jpeg);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(|error| error.to_string())?;
    let (width, height) = decoder.dimensions();
    // Bound the final RGBA allocation as well as the decoder's RGB buffer.
    if u64::from(width) * u64::from(height) > 32 * 1024 * 1024 {
        return Err("JPEG wallpaper exceeds the decoded pixel limit".into());
    }
    let pixels = image::DynamicImage::from_decoder(decoder)
        .map_err(|error| error.to_string())?
        .into_rgba8()
        .into_raw();
    Ok(BackgroundImageData {
        pixels,
        format: Fourcc::Abgr8888,
        size: Size::<i32, Logical>::from((width as i32, height as i32)),
    })
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
#[path = "background_tests.rs"]
mod tests;
