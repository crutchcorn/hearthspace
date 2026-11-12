//! Shared Controls paint contract. Behavior adapters must reuse these backdrops.
use super::ButtonVariant;
use crate::{
    Palette, SettingsButton,
    settings_material::{self as material, Interaction},
};
use xilem::{
    Color,
    masonry::{
        imaging::Painter,
        kurbo::{Rect, Stroke},
        peniko::Gradient,
    },
};

pub const BUTTON_HEIGHT: f64 = 36.;
pub const FIELD_HEIGHT: f64 = 42.;
pub const BAR_HEIGHT: f64 = 8.;

pub fn button(
    painter: &mut Painter<'_>,
    bounds: Rect,
    variant: ButtonVariant,
    state: Interaction,
    palette: &Palette,
) {
    if variant == ButtonVariant::Primary && state.disabled {
        let shape = bounds.to_rounded_rect(10.);
        painter
            .fill(shape, palette.accent.multiply_alpha(0.5))
            .draw();
        painter
            .stroke(
                shape,
                &Stroke::new(1.),
                palette.accent_border.multiply_alpha(0.5),
            )
            .draw();
        return;
    }
    if variant != ButtonVariant::Danger {
        material::paint_button_with_palette(
            painter,
            bounds,
            10.,
            match variant {
                ButtonVariant::Primary => SettingsButton::Primary,
                ButtonVariant::Quiet => SettingsButton::Bare,
                _ => SettingsButton::Control,
            },
            false,
            state,
            palette,
        );
        return;
    }
    let alpha = if state.disabled { 0.5 } else { 1. };
    let color = if (state.hovered || state.focused) && !state.disabled {
        Color::from_rgb8(0xd1, 0x48, 0x3e)
    } else {
        Color::from_rgb8(0xba, 0x3b, 0x32)
    }
    .multiply_alpha(alpha);
    painter.fill(bounds.to_rounded_rect(10.), color).draw();
    if state.focused && !state.disabled {
        painter
            .stroke(
                bounds.inflate(3., 3.).to_rounded_rect(13.),
                &Stroke::new(2.),
                palette.focus_ring,
            )
            .draw();
    }
}

pub fn field(
    painter: &mut Painter<'_>,
    bounds: Rect,
    invalid: bool,
    state: Interaction,
    palette: &Palette,
) {
    let alpha = if state.disabled { 0.5 } else { 1. };
    let shape = bounds.inset(-0.5).to_rounded_rect(9.5);
    painter
        .fill(shape, palette.surface_control.multiply_alpha(alpha))
        .draw();
    painter
        .stroke(
            shape,
            &Stroke::new(1.),
            if invalid {
                Color::from_rgb8(197, 72, 65)
            } else {
                palette.chrome_border
            }
            .multiply_alpha(alpha),
        )
        .draw();
    if state.focused && !state.disabled {
        painter
            .stroke(
                bounds.inflate(3., 3.).to_rounded_rect(13.),
                &Stroke::new(2.),
                palette.focus_ring,
            )
            .draw();
    }
}

/// Fractions are normalized by the owner; None keeps unknown progress semantic.
pub fn level_bar(
    painter: &mut Painter<'_>,
    bounds: Rect,
    fraction: Option<f64>,
    disabled: bool,
    palette: &Palette,
) {
    let alpha = if disabled { 0.5 } else { 1. };
    let y = bounds.center().y;
    let track = Rect::new(bounds.x0, y - 4., bounds.x1, y + 4.).to_rounded_rect(4.);
    painter
        .fill(track, palette.control_track.multiply_alpha(alpha))
        .draw();
    let end = bounds.x0 + bounds.width() * fraction.unwrap_or(1.).clamp(0., 1.);
    if end > bounds.x0 {
        let fill = Rect::new(bounds.x0, y - 4., end, y + 4.).to_rounded_rect(4.);
        let gradient = Gradient::new_linear((bounds.x0, 0.), (end, 0.)).with_stops([
            palette.accent_light.multiply_alpha(alpha),
            palette.accent.multiply_alpha(alpha),
        ]);
        painter.fill(fill, &gradient).draw();
    }
}

pub fn button_foreground(palette: &Palette, variant: ButtonVariant, disabled: bool) -> Color {
    match variant {
        ButtonVariant::Danger => Color::WHITE,
        ButtonVariant::Primary => palette.accent_ink,
        ButtonVariant::Quiet => palette.muted,
        ButtonVariant::Neutral => palette.text,
    }
    .multiply_alpha(if disabled { 0.5 } else { 1. })
}
