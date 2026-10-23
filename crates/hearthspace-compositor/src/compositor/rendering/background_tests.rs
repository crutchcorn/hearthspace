use hearthspace_ipc::BackgroundMode;
use smithay::utils::{Logical, Rectangle, Size};

use crate::geometry::CanvasPoint;

use super::{
    background_tile_axis_bounds, default_background_image_data, fixed_background_rects,
    wallpaper_presentation_rect,
};

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
fn canvas_background_bounds_follow_viewport_pan_and_zoom() {
    let output = Rectangle::<i32, Logical>::new((0, 0).into(), (800, 600).into());

    assert_ne!(
        super::background_image_visible_bounds(output, CanvasPoint { x: 0, y: 0 }, 1.0,),
        super::background_image_visible_bounds(
            output,
            CanvasPoint {
                x: 10_000,
                y: -5_000
            },
            0.25,
        )
    );
}

#[test]
fn stretch_background_uses_full_image_and_output() {
    let image = Size::<i32, Logical>::from((400, 200));
    let output = Rectangle::<i32, Logical>::new((10, 20).into(), (800, 600).into());

    assert_eq!(
        fixed_background_rects(image, output, BackgroundMode::Stretch),
        Some((
            Rectangle::from_size(Size::<f64, Logical>::from((400.0, 200.0))),
            output,
        ))
    );
}

#[test]
fn contains_background_preserves_aspect_inside_output() {
    let image = Size::<i32, Logical>::from((400, 200));
    let output = Rectangle::<i32, Logical>::new((10, 20).into(), (800, 600).into());

    assert_eq!(
        fixed_background_rects(image, output, BackgroundMode::Contains),
        Some((
            Rectangle::from_size(Size::<f64, Logical>::from((400.0, 200.0))),
            Rectangle::new((10, 120).into(), (800, 400).into()),
        ))
    );
}

#[test]
fn cover_background_crops_source_to_fill_output() {
    let image = Size::<i32, Logical>::from((400, 200));
    let output = Rectangle::<i32, Logical>::new((10, 20).into(), (300, 300).into());

    assert_eq!(
        fixed_background_rects(image, output, BackgroundMode::Cover),
        Some((
            Rectangle::new((100.0, 0.0).into(), (200.0, 200.0).into()),
            output,
        ))
    );
}

#[test]
fn wallpaper_presentation_rect_uses_physical_output_size() {
    let rect = Rectangle::<i32, Logical>::new((960, 0).into(), (960, 540).into());

    assert_eq!(
        wallpaper_presentation_rect(rect, 2),
        Rectangle::new((960, 0).into(), (1920, 1080).into())
    );
}

#[test]
fn default_grid_spacing_scales_about_each_output_origin() {
    for origin in [(0, 0), (-1280, 120), (960, -400)] {
        for scale in [1, 2, 3] {
            let first = super::grid_physical_point(
                (f64::from(origin.0 + 14), f64::from(origin.1 + 14)).into(),
                origin.into(),
                scale,
            );
            let next = super::grid_physical_point(
                (f64::from(origin.0 + 42), f64::from(origin.1 + 42)).into(),
                origin.into(),
                scale,
            );
            assert_eq!(next - first, (28 * scale, 28 * scale).into());
            assert_eq!(
                (first.x - origin.0, first.y - origin.1),
                (14 * scale, 14 * scale)
            );
        }
    }
}

#[test]
fn default_background_is_a_seamless_muted_dot_field() {
    let image = default_background_image_data();
    assert!(image.logical_grid);
    assert_eq!(image.size, Size::<i32, Logical>::from((1792, 1008)));

    assert_eq!(
        pixel_rgba(&image.pixels, image.size, 0, 0),
        [229, 233, 227, 255]
    );
    assert_eq!(
        pixel_rgba(&image.pixels, image.size, 1791, 0),
        [229, 233, 227, 255]
    );
    assert_eq!(
        pixel_rgba(&image.pixels, image.size, 14, 14),
        [208, 215, 209, 255]
    );
}

fn pixel_rgba(pixels: &[u8], size: Size<i32, Logical>, x: i32, y: i32) -> [u8; 4] {
    let offset = ((y * size.w + x) * 4) as usize;
    pixels[offset..offset + 4].try_into().unwrap()
}
