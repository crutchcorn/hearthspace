use super::{Interaction, SettingsButton, SettingsSurface};
use crate::Palette;
use xilem::{
    Color,
    masonry::{
        imaging::{BlurredRoundedRect, Composite, Painter},
        kurbo::{Affine, BezPath, Rect, Shape as _, Stroke},
        peniko::Gradient,
    },
};

/// Repeat CSS-style radial dots from the background's origin. The caller clips
/// the pattern to its surface. Integer enclosing rectangles avoid adding a
/// second antialiasing fade to the gradient's own transparent edge.
pub fn dot_pattern(
    painter: &mut Painter<'_>,
    bounds: Rect,
    spacing: f64,
    solid_radius: f64,
    fade_radius: f64,
    color: Color,
) {
    if !(spacing > 0.0 && fade_radius > 0.0) {
        return;
    }
    let solid_stop = (solid_radius / fade_radius).clamp(0.0, 1.0) as f32;
    let mut x = bounds.x0 + spacing / 2.0;
    while x < bounds.x1 + fade_radius {
        let mut y = bounds.y0 + spacing / 2.0;
        while y < bounds.y1 + fade_radius {
            // Align the radial paint with the reference's rasterized tile centers.
            let center = (x - 0.5, y - 0.5);
            let gradient = Gradient::new_radial(center, fade_radius as f32).with_stops([
                (0.0, color),
                (solid_stop, color),
                (1.0, color.multiply_alpha(0.0)),
            ]);
            painter
                .fill(
                    Rect::new(
                        (center.0 - fade_radius).floor(),
                        (center.1 - fade_radius).floor(),
                        (center.0 + fade_radius).ceil(),
                        (center.1 + fade_radius).ceil(),
                    ),
                    &gradient,
                )
                .draw();
            y += spacing;
        }
        x += spacing;
    }
}

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
pub fn edge_light_with_palette(
    painter: &mut Painter<'_>,
    rect: Rect,
    radius: f64,
    palette: &Palette,
) {
    let gradient = Gradient::new_radial((0.0, 0.0), 1.0).with_stops([
        (0.0, palette.edge_light),
        (0.4, palette.edge_light.with_alpha(0.0)),
        (1.0, palette.edge_light.with_alpha(0.0)),
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
pub fn glass_edge_with_palette(
    painter: &mut Painter<'_>,
    rect: Rect,
    radius: f64,
    active: bool,
    palette: &Palette,
) {
    let shape = rect.inset(-1.0).to_rounded_rect((radius - 1.0).max(0.0));
    let top = if active {
        palette.glass_edge_active_top
    } else {
        palette.glass_edge_top
    };
    painter.with_fill_clip(shape, |painter| {
        for (offset, color) in [(1.0, top), (-1.0, palette.glass_edge_bottom)] {
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
            .stroke(shape, &Stroke::new(1.0), palette.glass_edge_active_border)
            .draw();
    }
}

pub fn paint_surface_with_palette(
    painter: &mut Painter<'_>,
    rect: Rect,
    radius: f64,
    kind: SettingsSurface,
    palette: &Palette,
) {
    if kind == SettingsSurface::Raised {
        shadow(
            painter,
            rect,
            radius,
            palette.panel_shadow,
            (0.0, 24.0),
            72.0,
            -22.0,
        );
        shadow(
            painter,
            rect,
            radius,
            palette.panel_accent_shadow,
            (0.0, 6.0),
            24.0,
            -8.0,
        );
    }
    let (fill, border) = match kind {
        SettingsSurface::Control => (palette.surface_control, palette.border),
        SettingsSurface::Subtle => (palette.surface_subtle, palette.border),
        SettingsSurface::Raised => (palette.chrome_raised, palette.chrome_border),
    };
    let shape = rect.inset(-0.5).to_rounded_rect(radius);
    if kind == SettingsSurface::Raised {
        // Masonry layers cannot backdrop-filter siblings already painted by the
        // client. Approximate the blurred window tint with an opaque backing,
        // then retain the reference glass fill and edge lighting on top.
        painter.fill(shape, palette.popup_backing).draw();
    }
    painter.fill(shape, fill).draw();
    if kind == SettingsSurface::Raised {
        edge_light_with_palette(painter, rect, radius, palette);
    }
    painter.stroke(shape, &Stroke::new(1.0), border).draw();
    glass_edge_with_palette(painter, rect, radius, false, palette);
}

pub fn paint_button_with_palette(
    painter: &mut Painter<'_>,
    rect: Rect,
    radius: f64,
    kind: SettingsButton,
    selected: bool,
    interaction: Interaction,
    palette: &Palette,
) {
    let emphasized = !interaction.disabled && (interaction.hovered || interaction.active);
    let glow = emphasized || selected || (kind == SettingsButton::Primary && !interaction.disabled);
    if glow {
        shadow(
            painter,
            rect,
            radius,
            palette.control_shadow,
            (0.0, 6.0),
            if palette.dark { 24.0 } else { 22.0 },
            -5.0,
        );
    }
    let mut fill = match kind {
        SettingsButton::Bare | SettingsButton::Outline => Color::TRANSPARENT,
        _ => palette.surface_control,
    };
    let mut border = match kind {
        SettingsButton::Bare => Color::TRANSPARENT,
        SettingsButton::Card => palette.chrome_border,
        _ => palette.border,
    };
    if emphasized {
        fill = palette.button_hover;
        border = if kind == SettingsButton::Bare {
            palette.chrome_border
        } else {
            palette.accent_border
        };
    }
    if selected {
        fill = palette.accent_soft;
        border = if kind == SettingsButton::Card {
            palette.accent
        } else {
            palette.accent_border
        };
    }
    if kind == SettingsButton::Primary && !interaction.disabled {
        fill = if emphasized {
            palette.accent_hover
        } else {
            palette.accent
        };
        border = palette.accent_border;
    }
    if kind == SettingsButton::Primary && interaction.disabled {
        border = palette.chrome_border;
    }
    let shape = rect.inset(-0.5).to_rounded_rect(radius);
    painter.fill(shape, fill).draw();
    if glow {
        let wash = Gradient::new_radial(
            rect.center(),
            (rect.width().max(rect.height()) / 2.0) as f32,
        )
        .with_stops([(0.0, Color::TRANSPARENT), (1.0, palette.control_inset_glow)]);
        painter.fill(shape, &wash).draw();
    }
    painter.stroke(shape, &Stroke::new(1.0), border).draw();
    if (kind != SettingsButton::Bare || emphasized || selected)
        && !(kind == SettingsButton::Primary && interaction.disabled)
    {
        glass_edge_with_palette(painter, rect, radius, emphasized || selected, palette);
    }
    if interaction.focused && !interaction.disabled {
        painter
            .stroke(
                rect.inflate(3.0, 3.0).to_rounded_rect(radius + 3.0),
                &Stroke::new(2.0),
                palette.focus_ring,
            )
            .draw();
    }
}
