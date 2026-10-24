use hearthspace_ipc::BackgroundMode;
use hearthspace_ui_components::Palette;
use image::{
    Rgba, RgbaImage,
    imageops::{self, FilterType},
};

/// Work at at most 1920px and scale the finished softened image on larger
/// outputs. Bounds and decode limits keep uploads from stalling authentication.
pub(super) fn compose(
    source: &RgbaImage,
    fit: BackgroundMode,
    width: u32,
    height: u32,
    palette: Palette,
) -> RgbaImage {
    let reduction = (width.max(height) as f64 / 1920.0).max(1.0);
    let (width, height) = (
        (width as f64 / reduction).round().max(1.0) as u32,
        (height as f64 / reduction).round().max(1.0) as u32,
    );
    let c = palette.app_background.to_rgba8();
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([c.r, c.g, c.b, 255]));
    let (sw, sh) = source.dimensions();
    if sw == 0 || sh == 0 {
        return canvas;
    }
    let (fw, fh) = (width as f64 / sw as f64, height as f64 / sh as f64);
    let factor = match fit {
        BackgroundMode::Cover => fw.max(fh),
        BackgroundMode::Contains => fw.min(fh),
        BackgroundMode::RepeatWithCanvas => 1.0 / reduction,
        BackgroundMode::Stretch => 1.0,
    };
    let (target_w, target_h) = if fit == BackgroundMode::Stretch {
        (width, height)
    } else {
        (
            (sw as f64 * factor).round().clamp(1.0, 16384.0) as u32,
            (sh as f64 * factor).round().clamp(1.0, 16384.0) as u32,
        )
    };
    let resized = if fit == BackgroundMode::Cover {
        // Crop in source space before scaling: a one-pixel-wide panorama must
        // not allocate an enormous intermediate image just to cover the screen.
        let ratio = f64::from(width) / f64::from(height);
        let crop_w = sw.min((f64::from(sh) * ratio).round().max(1.0) as u32);
        let crop_h = sh.min((f64::from(sw) / ratio).round().max(1.0) as u32);
        let crop = imageops::crop_imm(source, (sw - crop_w) / 2, (sh - crop_h) / 2, crop_w, crop_h)
            .to_image();
        imageops::resize(&crop, width, height, FilterType::Triangle)
    } else {
        imageops::resize(source, target_w, target_h, FilterType::Triangle)
    };
    let (target_w, target_h) = resized.dimensions();
    let (x, y) = (
        (i64::from(width) - i64::from(target_w)) / 2,
        (i64::from(height) - i64::from(target_h)) / 2,
    );
    if fit == BackgroundMode::RepeatWithCanvas {
        let (start_x, start_y) = (
            x.rem_euclid(i64::from(target_w)) - i64::from(target_w),
            y.rem_euclid(i64::from(target_h)) - i64::from(target_h),
        );
        for py in (start_y..i64::from(height)).step_by(target_h as usize) {
            for px in (start_x..i64::from(width)).step_by(target_w as usize) {
                imageops::overlay(&mut canvas, &resized, px, py);
            }
        }
    } else {
        imageops::overlay(&mut canvas, &resized, x, y);
    }
    let mut blurred = imageops::blur(&canvas, (18.0 / reduction) as f32);
    for pixel in blurred.pixels_mut() {
        let gray = f64::from(pixel[0]) * 0.2126
            + f64::from(pixel[1]) * 0.7152
            + f64::from(pixel[2]) * 0.0722;
        for (channel, field) in pixel.0[..3].iter_mut().zip([c.r, c.g, c.b]) {
            let saturated = f64::from(*channel) * 0.85 + gray * 0.15;
            *channel = (saturated * 0.32 + f64::from(field) * 0.68).round() as u8;
        }
        pixel[3] = 255;
    }
    blurred
}

#[cfg(test)]
#[path = "photo_tests.rs"]
mod tests;
