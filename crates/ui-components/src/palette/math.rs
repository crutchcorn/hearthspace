use xilem::Color;

/// CSS color-mix(in srgb), including premultiplied alpha, quantized like the
/// existing native tokens so the default orange/light pixels remain unchanged.
pub(super) fn mix(first: Color, weight: f32, second: Color) -> Color {
    let a = first.components;
    let b = second.components;
    let alpha = a[3] * weight + b[3] * (1.0 - weight);
    let channel = |index: usize| {
        if alpha == 0.0 {
            0
        } else {
            ((a[index] * a[3] * weight + b[index] * b[3] * (1.0 - weight)) / alpha * 255.0).round()
                as u8
        }
    };
    Color::from_rgba8(
        channel(0),
        channel(1),
        channel(2),
        (alpha * 255.0).round() as u8,
    )
}

pub(super) fn alpha(color: Color, opacity: f32) -> Color {
    let [r, g, b, _] = color.components;
    Color::from_rgba8(
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
        (opacity * 255.0).round() as u8,
    )
}

pub(super) fn over(foreground: Color, background: Color) -> Color {
    let f = foreground.components;
    let b = background.components;
    Color::from_rgb8(
        ((f[0] * f[3] + b[0] * (1.0 - f[3])) * 255.0).round() as u8,
        ((f[1] * f[3] + b[1] * (1.0 - f[3])) * 255.0).round() as u8,
        ((f[2] * f[3] + b[2] * (1.0 - f[3])) * 255.0).round() as u8,
    )
}

pub(super) fn contrast(first: Color, second: Color) -> f64 {
    fn luminance(color: Color) -> f64 {
        color.components[..3]
            .iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(&c, weight)| {
                let c = f64::from(c);
                weight
                    * if c <= 0.04045 {
                        c / 12.92
                    } else {
                        ((c + 0.055) / 1.055).powf(2.4)
                    }
            })
            .sum()
    }
    let a = luminance(first);
    let b = luminance(second);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Shell foregrounds retain contrast against the worst-case translucent panel
/// backdrop. Reference Settings ink remains separate and is never altered here.
pub(super) fn glass_ink(seed: Color, dark: bool, surfaces: &[Color]) -> Color {
    let endpoint = if dark { Color::WHITE } else { Color::BLACK };
    for step in 0..=100 {
        let candidate = mix(seed, 1.0 - step as f32 / 100.0, endpoint);
        if surfaces
            .iter()
            .all(|surface| contrast(candidate, *surface) >= 4.5)
        {
            return candidate;
        }
    }
    endpoint
}
