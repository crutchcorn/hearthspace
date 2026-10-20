//! Orange/light materials from hearthspace-ui-experiments/src/index.css.
//! Translucent tokens are separate from opaque backing colors so composition
//! remains the caller's choice. Other accent choices do not change this theme.
use xilem::Color;

pub const APP_BACKGROUND: Color = Color::from_rgb8(0xe5, 0xe9, 0xe3);
pub const SURFACE: Color = Color::from_rgb8(0xf6, 0xf4, 0xef);
pub const SURFACE_RAISED: Color = Color::from_rgb8(0xfa, 0xf9, 0xf6);
pub const CHROME: Color = Color::from_rgba8(0xf6, 0xf4, 0xef, 138);
pub const CHROME_RAISED: Color = Color::from_rgba8(0xfa, 0xf9, 0xf6, 209);
pub const CHROME_BORDER: Color = Color::from_rgba8(0xff, 0xff, 0xff, 112);
pub const PAPER: Color = Color::from_rgba8(0xf6, 0xf4, 0xef, 77);
pub const SURFACE_SUBTLE: Color = Color::from_rgba8(0xff, 0xff, 0xff, 20);
pub const SURFACE_CONTROL: Color = Color::from_rgba8(0xff, 0xff, 0xff, 41);
pub const BORDER: Color = Color::from_rgba8(0x41, 0x57, 0x54, 28);
pub const TEXT: Color = Color::from_rgb8(0x30, 0x3a, 0x3c);
// The reference's #667373 falls below 3:1 when glass covers a black window.
// This deeper ink retains 4.5:1 over our 82% warm glass backing.
pub const TEXT_MUTED: Color = Color::from_rgb8(0x4c, 0x55, 0x53);
pub const ACCENT: Color = Color::from_rgb8(0xe9, 0x64, 0x2a);
// Accent foregrounds need a deeper orange than the reference's decorative fill
// to retain small-text contrast when translucent chrome covers a black window.
pub const ACCENT_STRONG: Color = Color::from_rgb8(0x8c, 0x38, 0x18);
pub const ACCENT_INK: Color = Color::from_rgb8(0xff, 0xfa, 0xf5);
pub const ACCENT_BORDER: Color = Color::from_rgba8(0xf3, 0xad, 0x8e, 174);
pub const ACCENT_SOFT: Color = Color::from_rgba8(0xf5, 0xbb, 0xa1, 58);
pub const ACCENT_WASH: Color = Color::from_rgba8(0xf4, 0xb3, 0x97, 36);
pub const ACCENT_SUBTLE: Color = ACCENT_SOFT;
pub const BUTTON_HOVER: Color = Color::from_rgba8(0xff, 0xff, 0xff, 92);
pub const RAISED_BUTTON_HOVER: Color = Color::from_rgba8(0xfa, 0xdf, 0xd3, 61);
pub const BUTTON_ACTIVE: Color = ACCENT_SOFT;
pub const CONTROL_TRACK: Color = Color::from_rgba8(0x53, 0x68, 0x60, 64);
pub const CONTROL_THUMB: Color = Color::from_rgb8(0xf6, 0xfa, 0xf7);
pub const DANGER_SUBTLE: Color = Color::from_rgb8(0xfd, 0xee, 0xed);

pub const RADIUS_DETAIL: f64 = 5.0;
pub const RADIUS_CONTROL: f64 = 10.0;
pub const RADIUS_TILE: f64 = 14.0;
pub const RADIUS_PANEL: f64 = 20.0;

/// Controls in a raised toolbar use a faint accent hover instead of white.
pub const TRANSLUCENT_FOCUS_CLASS: &str = "hearthspace-translucent-focus";

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(rgb: [f64; 3]) -> f64 {
        rgb.into_iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(value, weight)| {
                weight
                    * if value <= 0.04045 {
                        value / 12.92
                    } else {
                        ((value + 0.055) / 1.055).powf(2.4)
                    }
            })
            .sum()
    }

    #[test]
    fn small_text_retains_contrast_when_glass_covers_a_black_window() {
        for background in [SURFACE.with_alpha(0.82), CHROME_RAISED] {
            let [red, green, blue, alpha] = background.components;
            let backdrop = [red, green, blue].map(|value| f64::from(value * alpha));
            let selected = std::array::from_fn(|index| {
                f64::from(ACCENT_SOFT.components[index] * ACCENT_SOFT.components[3])
                    + backdrop[index] * f64::from(1.0 - ACCENT_SOFT.components[3])
            });
            for surface in [backdrop, selected] {
                for foreground in [TEXT, TEXT_MUTED, ACCENT_STRONG] {
                    let [red, green, blue, _] = foreground.components;
                    let ink = luminance([red, green, blue].map(f64::from));
                    let contrast = (luminance(surface) + 0.05) / (ink + 0.05);
                    assert!(
                        contrast >= 4.5,
                        "{foreground:?} over {background:?}: {contrast}"
                    );
                }
            }
        }
    }
}
