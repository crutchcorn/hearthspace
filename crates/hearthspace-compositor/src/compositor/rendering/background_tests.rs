use hearthspace_ipc::BackgroundMode;
use smithay::utils::{Logical, Rectangle, Size};

use crate::geometry::CanvasPoint;

use super::{background_tile_axis_bounds, fixed_background_rects, wallpaper_presentation_rect};

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
