//! The compact switch and full-card row share the same track material.
use hearthspace_ui_components as components;
use xilem::masonry::{
    imaging::Painter,
    kurbo::{Circle, Rect, Stroke},
    properties::types::Gradient,
};

pub(super) fn paint(
    palette: components::Palette,
    painter: &mut Painter<'_>,
    x: f64,
    y: f64,
    on: bool,
    disabled: bool,
    focused: bool,
) {
    let track = Rect::new(x + 0.5, y + 0.5, x + 39.5, y + 23.5).to_rounded_rect(12.0);
    let alpha = if disabled { 0.5 } else { 1.0 };
    if on {
        if palette.dark {
            // Settings.module.css: color-mix(in srgb, accent-base 72%, surface-tint).
            let [r, g, b] = std::array::from_fn(|channel| {
                ((palette.accent_base.components[channel] * 0.72
                    + palette.surface.components[channel] * 0.28)
                    * 255.0)
                    .round() as u8
            });
            painter
                .fill(
                    track,
                    xilem::Color::from_rgb8(r, g, b).multiply_alpha(alpha),
                )
                .draw();
        } else {
            let gradient = Gradient::new_linear(135.0_f64.to_radians())
                .with_stops([
                    palette.accent_bright.multiply_alpha(alpha),
                    palette.accent.multiply_alpha(alpha),
                ])
                .get_peniko_gradient_for_rect(track.rect());
            painter.fill(track, &gradient).draw();
        }
        let edge = if palette.dark {
            components::Palette {
                glass_edge_top: xilem::Color::from_rgba8(255, 255, 255, 0x14).multiply_alpha(alpha),
                glass_edge_bottom: xilem::Color::TRANSPARENT,
                ..palette
            }
        } else {
            palette
        };
        components::settings_material::glass_edge_with_palette(
            painter,
            track.rect(),
            12.0,
            false,
            &edge,
        );
    } else {
        painter
            .fill(track, palette.control_track.multiply_alpha(alpha))
            .draw();
    }
    painter
        .stroke(
            track,
            &Stroke::new(1.0),
            if on {
                palette.accent_border
            } else {
                palette.chrome_border
            }
            .multiply_alpha(alpha),
        )
        .draw();
    let center = (x + if on { 28.0 } else { 12.0 }, y + 12.0);
    components::settings_material::shadow(
        painter,
        Rect::new(
            center.0 - 8.0,
            center.1 - 8.0,
            center.0 + 8.0,
            center.1 + 8.0,
        ),
        8.0,
        palette.border.multiply_alpha(alpha),
        (0.0, 1.0),
        3.0,
        0.0,
    );
    painter
        .fill(
            Circle::new(center, 8.0),
            palette.control_thumb.multiply_alpha(alpha),
        )
        .draw();
    if focused {
        painter
            .stroke(
                Rect::new(x - 3.0, y - 3.0, x + 43.0, y + 27.0).to_rounded_rect(15.0),
                &Stroke::new(2.0),
                palette.focus_ring,
            )
            .draw();
    }
}
