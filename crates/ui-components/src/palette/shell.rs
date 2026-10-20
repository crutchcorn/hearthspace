//! Contrast against the actual shell layers, including the launcher tab bar.
use super::{
    Palette,
    math::{alpha, glass_ink, over},
};
use xilem::Color;

pub(super) fn resolve_ink(palette: &mut Palette) {
    let backdrop = if palette.dark {
        Color::WHITE
    } else {
        Color::BLACK
    };
    let mut backgrounds = Vec::with_capacity(16);
    for material in [palette.chrome_raised, alpha(palette.surface, 0.82)] {
        let panel = over(material, backdrop);
        let header = over(palette.surface_control, panel);
        let tabs = over(palette.surface_subtle, header);
        let row = over(palette.surface_subtle, panel);
        backgrounds.extend([
            panel,
            header,
            tabs,
            row,
            over(palette.accent_soft, panel),
            over(palette.accent_soft, row),
            over(palette.accent_soft, tabs),
            over(palette.button_hover_on_glass, tabs),
        ]);
    }
    palette.text_on_glass = glass_ink(palette.text, palette.dark, &backgrounds);
    palette.text_muted = glass_ink(palette.text_muted, palette.dark, &backgrounds);
    palette.accent_strong = glass_ink(palette.accent_strong, palette.dark, &backgrounds);
    palette.focus_ring_on_glass = palette.accent_strong;
}
