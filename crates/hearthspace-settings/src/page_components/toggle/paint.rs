//! The compact switch and full-card row share the same track material.
use hearthspace_ui_components as components;
use xilem::masonry::{
    imaging::Painter,
    kurbo::{Circle, Rect, Stroke},
    properties::types::Gradient,
};

pub(super) fn paint(
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
        let gradient = Gradient::new_linear(135.0_f64.to_radians())
            .with_stops([
                components::settings_material::ACCENT_BRIGHT.multiply_alpha(alpha),
                components::ACCENT.multiply_alpha(alpha),
            ])
            .get_peniko_gradient_for_rect(track.rect());
        painter.fill(track, &gradient).draw();
        components::settings_material::glass_edge(painter, track.rect(), 12.0, false);
    } else {
        painter
            .fill(track, components::CONTROL_TRACK.multiply_alpha(alpha))
            .draw();
    }
    painter
        .stroke(
            track,
            &Stroke::new(1.0),
            if on {
                components::ACCENT_BORDER
            } else {
                components::CHROME_BORDER
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
        components::BORDER.multiply_alpha(alpha),
        (0.0, 1.0),
        3.0,
        0.0,
    );
    painter
        .fill(
            Circle::new(center, 8.0),
            components::CONTROL_THUMB.multiply_alpha(alpha),
        )
        .draw();
    if focused {
        painter
            .stroke(
                Rect::new(x - 3.0, y - 3.0, x + 43.0, y + 27.0).to_rounded_rect(15.0),
                &Stroke::new(2.0),
                components::FOCUS_RING,
            )
            .draw();
    }
}
