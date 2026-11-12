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

fn jpeg(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([30, 100, 200]));
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 100)
        .encode_image(&image)
        .unwrap();
    bytes
}

#[test]
fn jpeg_wallpapers_decode_to_opaque_rgba() {
    let data = super::decode_background_image(&jpeg(8, 4)).unwrap();
    assert_eq!(data.size, Size::<i32, Logical>::from((8, 4)));
    assert_eq!(data.format, smithay::backend::allocator::Fourcc::Abgr8888);
    assert_eq!(data.pixels.len(), 8 * 4 * 4);
    for pixel in data.pixels.as_chunks::<4>().0 {
        for (actual, expected) in pixel[..3].iter().zip([30u8, 100, 200]) {
            assert!(actual.abs_diff(expected) <= 3, "JPEG pixel: {pixel:?}");
        }
        assert_eq!(pixel[3], 255);
    }
}

#[test]
fn invalid_and_oversized_jpeg_wallpapers_are_rejected() {
    assert!(super::decode_background_image(b"\xff\xd8\xffnot a JPEG").is_err());
    assert!(super::decode_background_image(&jpeg(16_385, 1)).is_err());
    assert!(super::decode_background_image(b"GIF89a").is_err());
}

#[test]
fn png_wallpapers_keep_their_rgba_pixels() {
    let pixels = [30, 100, 200, 255, 200, 30, 100, 80];
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
    let data = super::decode_background_image(&bytes).unwrap();
    assert_eq!(data.size, Size::<i32, Logical>::from((2, 1)));
    assert_eq!(data.pixels, pixels);
}
