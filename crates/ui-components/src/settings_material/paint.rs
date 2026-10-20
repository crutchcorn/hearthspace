use super::{Interaction, SettingsButton, SettingsSurface};
use xilem::{
    Color,
    masonry::{
        imaging::{BlurredRoundedRect, Composite, Painter},
        kurbo::{Affine, BezPath, Rect, Shape as _, Stroke},
        peniko::Gradient,
    },
};

/// CSS drop shadow: blur is the CSS blur radius, spread may be negative.
pub fn shadow(
    painter: &mut Painter<'_>,
    rect: Rect,
    radius: f64,
    color: Color,
    offset: (f64, f64),
    blur: f64,
    spread: f64,
) {
    let shape = rect.to_rounded_rect(radius);
    let source = rect.inflate(spread, spread);
    if source.width() <= 0.0 || source.height() <= 0.0 {
        return;
    }
    let extent = blur * 2.0 + offset.0.abs().max(offset.1.abs()) + spread.abs();
    let outside = BezPath::from_iter(
        rect.inflate(extent, extent)
            .path_elements(0.1)
            .chain(shape.to_path(0.1).reverse_subpaths()),
    );
    // CSS shadows do not show through a translucent element's own interior.
    painter.with_fill_clip(outside, |painter| {
        painter.blurred_rounded_rect(BlurredRoundedRect {
            transform: Affine::translate(offset),
            rect: source,
            color,
            radius: (radius + spread).max(0.0),
            std_dev: blur / 2.0,
            composite: Composite::default(),
        });
    });
}

/// The reference's top-left ellipse, fading to transparent at 40%.
pub fn edge_light(painter: &mut Painter<'_>, rect: Rect, radius: f64) {
    let gradient = Gradient::new_radial((0.0, 0.0), 1.0).with_stops([
        (0.0, crate::ACCENT.with_alpha(0.05)),
        (0.4, crate::ACCENT.with_alpha(0.0)),
        (1.0, crate::ACCENT.with_alpha(0.0)),
    ]);
    painter.with_fill_clip(rect.to_rounded_rect(radius), |painter| {
        painter
            .fill(rect, &gradient)
            .brush_transform(Some(
                Affine::translate((rect.x0, rect.y0))
                    * Affine::scale_non_uniform(
                        rect.width().max(1.0) * 2.0_f64.sqrt(),
                        rect.height().max(1.0) * 2.0_f64.sqrt(),
                    ),
            ))
            .draw();
    });
}

/// Inset one-pixel glass highlights, clipped to the rounded material.
pub fn glass_edge(painter: &mut Painter<'_>, rect: Rect, radius: f64, active: bool) {
    let shape = rect.inset(-1.0).to_rounded_rect((radius - 1.0).max(0.0));
    let top = if active { 199 } else { 133 };
    painter.with_fill_clip(shape, |painter| {
        for (offset, color) in [
            (1.0, Color::from_rgba8(255, 255, 255, top)),
            (-1.0, Color::from_rgba8(0x49, 0x63, 0x5b, 10)),
        ] {
            let hole = shape + xilem::masonry::kurbo::Vec2::new(0.0, offset);
            let edge = BezPath::from_iter(
                rect.path_elements(0.1)
                    .chain(hole.to_path(0.1).reverse_subpaths()),
            );
            painter.fill(edge, color).draw();
        }
    });
    if active {
        painter
            .stroke(
                shape,
                &Stroke::new(1.0),
                Color::from_rgba8(255, 255, 255, 48),
            )
            .draw();
    }
}

pub fn paint_surface(painter: &mut Painter<'_>, rect: Rect, radius: f64, kind: SettingsSurface) {
    if kind == SettingsSurface::Raised {
        shadow(
            painter,
            rect,
            radius,
            Color::from_rgba8(0x35, 0x4e, 0x3b, 43),
            (0.0, 24.0),
            72.0,
            -22.0,
        );
        shadow(
            painter,
            rect,
            radius,
            crate::ACCENT.with_alpha(0.16),
            (0.0, 6.0),
            24.0,
            -8.0,
        );
    }
    let (fill, border) = match kind {
        SettingsSurface::Control => (crate::SURFACE_CONTROL, crate::BORDER),
        SettingsSurface::Subtle => (crate::SURFACE_SUBTLE, crate::BORDER),
        SettingsSurface::Raised => (crate::CHROME_RAISED, crate::CHROME_BORDER),
    };
    let shape = rect.inset(-0.5).to_rounded_rect(radius);
    painter.fill(shape, fill).draw();
    if kind == SettingsSurface::Raised {
        edge_light(painter, rect, radius);
    }
    painter.stroke(shape, &Stroke::new(1.0), border).draw();
    glass_edge(painter, rect, radius, false);
}

pub fn paint_button(
    painter: &mut Painter<'_>,
    rect: Rect,
    radius: f64,
    kind: SettingsButton,
    selected: bool,
    interaction: Interaction,
) {
    let emphasized = !interaction.disabled && (interaction.hovered || interaction.active);
    let glow = emphasized || selected || (kind == SettingsButton::Primary && !interaction.disabled);
    if glow {
        shadow(
            painter,
            rect,
            radius,
            crate::ACCENT.with_alpha(0.14),
            (0.0, 6.0),
            22.0,
            -5.0,
        );
    }
    let mut fill = match kind {
        SettingsButton::Bare => Color::TRANSPARENT,
        _ => crate::SURFACE_CONTROL,
    };
    let mut border = match kind {
        SettingsButton::Bare => Color::TRANSPARENT,
        SettingsButton::Card => crate::CHROME_BORDER,
        _ => crate::BORDER,
    };
    if emphasized {
        fill = crate::BUTTON_HOVER;
        border = if kind == SettingsButton::Bare {
            crate::CHROME_BORDER
        } else {
            crate::ACCENT_BORDER
        };
    }
    if selected {
        fill = crate::ACCENT_SOFT;
        border = if kind == SettingsButton::Card {
            crate::ACCENT
        } else {
            crate::ACCENT_BORDER
        };
    }
    if kind == SettingsButton::Primary && !interaction.disabled {
        fill = if emphasized {
            Color::from_rgb8(0xed, 0x80, 0x50)
        } else {
            crate::ACCENT
        };
        border = crate::ACCENT_BORDER;
    }
    let shape = rect.inset(-0.5).to_rounded_rect(radius);
    painter.fill(shape, fill).draw();
    if glow {
        let wash = Gradient::new_radial(
            rect.center(),
            (rect.width().max(rect.height()) / 2.0) as f32,
        )
        .with_stops([
            (0.0, Color::TRANSPARENT),
            (1.0, crate::ACCENT.with_alpha(0.04)),
        ]);
        painter.fill(shape, &wash).draw();
    }
    painter.stroke(shape, &Stroke::new(1.0), border).draw();
    if kind != SettingsButton::Bare || emphasized || selected {
        glass_edge(painter, rect, radius, emphasized || selected);
    }
    if interaction.focused && !interaction.disabled {
        painter
            .stroke(
                rect.inflate(3.0, 3.0).to_rounded_rect(radius + 3.0),
                &Stroke::new(2.0),
                crate::FOCUS_RING,
            )
            .draw();
    }
}
