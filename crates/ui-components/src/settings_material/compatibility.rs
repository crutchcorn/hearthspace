//! Default-palette helpers retained for standalone examples and static previews.
use super::{Interaction, SettingsButton, SettingsSurface, paint};
use crate::Palette;
use xilem::masonry::{core::Property, imaging::Painter, kurbo::Rect};

pub fn edge_light(painter: &mut Painter<'_>, rect: Rect, radius: f64) {
    paint::edge_light_with_palette(painter, rect, radius, Palette::static_default());
}
pub fn glass_edge(painter: &mut Painter<'_>, rect: Rect, radius: f64, active: bool) {
    paint::glass_edge_with_palette(painter, rect, radius, active, Palette::static_default());
}
pub fn paint_surface(painter: &mut Painter<'_>, rect: Rect, radius: f64, kind: SettingsSurface) {
    paint::paint_surface_with_palette(painter, rect, radius, kind, Palette::static_default());
}
pub fn paint_button(
    painter: &mut Painter<'_>,
    rect: Rect,
    radius: f64,
    kind: SettingsButton,
    selected: bool,
    interaction: Interaction,
) {
    paint::paint_button_with_palette(
        painter,
        rect,
        radius,
        kind,
        selected,
        interaction,
        Palette::static_default(),
    );
}
