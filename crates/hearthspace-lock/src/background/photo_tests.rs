use super::*;

#[test]
fn translucent_wallpaper_is_composited_over_an_opaque_field() {
    let palette = Palette::default();
    let source = RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 0]));
    let result = compose(&source, BackgroundMode::Stretch, 32, 24, palette);
    let field = palette.app_background.to_rgba8();
    for pixel in result.pixels() {
        assert_eq!(pixel[3], 255);
        // Saturation subtly affects the colored field, without revealing any
        // transparent source or contents behind the lock surface.
        for (actual, expected) in pixel.0[..3].iter().zip([field.r, field.g, field.b]) {
            assert!(actual.abs_diff(expected) <= 1);
        }
    }
}

#[test]
fn contain_preserves_field_margins_while_cover_and_stretch_fill() {
    let source = RgbaImage::from_pixel(40, 20, Rgba([0, 0, 0, 255]));
    let palette = Palette::default();
    let contain = compose(&source, BackgroundMode::Contains, 256, 256, palette);
    let cover = compose(&source, BackgroundMode::Cover, 256, 256, palette);
    let stretch = compose(&source, BackgroundMode::Stretch, 256, 256, palette);
    assert!(contain.get_pixel(128, 0)[0] > contain.get_pixel(128, 128)[0] + 50);
    assert_eq!(cover, stretch);
    assert!(contain.get_pixel(128, 128)[0].abs_diff(cover.get_pixel(128, 128)[0]) <= 1);
}

#[test]
fn cover_crops_extreme_aspect_ratios_before_resizing() {
    let source = RgbaImage::from_fn(1, 16384, |_, y| {
        if (8000..8400).contains(&y) {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    });
    let result = compose(&source, BackgroundMode::Cover, 64, 64, Palette::default());
    let black = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]));
    assert_eq!(
        result,
        compose(&black, BackgroundMode::Stretch, 64, 64, Palette::default())
    );
}

#[test]
fn repeat_fills_canvas_without_contain_margins_and_large_outputs_are_bounded() {
    let source = RgbaImage::from_pixel(4, 2, Rgba([0, 0, 0, 255]));
    let palette = Palette::default();
    assert_eq!(
        compose(&source, BackgroundMode::RepeatWithCanvas, 64, 64, palette),
        compose(&source, BackgroundMode::Stretch, 64, 64, palette),
    );
    let bounded = compose(&source, BackgroundMode::Cover, 7680, 8, palette);
    assert_eq!(bounded.dimensions(), (1920, 2));
}
