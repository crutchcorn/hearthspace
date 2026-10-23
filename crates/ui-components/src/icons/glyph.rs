//! Retained vector artwork for native controls that paint their own contents.
use super::{Geometry, Icon, optical_stroke};
use xilem::{
    Color,
    masonry::{
        imaging::Painter,
        kurbo::{Affine, Cap, Join, Rect, Stroke},
    },
};

/// The same SVG geometry as [`super::icon`], without a separate widget node.
pub struct IconGlyph(Geometry);

impl IconGlyph {
    pub fn new(icon: Icon, filled: bool) -> Self {
        Self(Geometry::new(icon, filled))
    }

    /// Paint an optional body tint beneath an outline document/folder glyph.
    /// Only the first closed body path is filled; detail strokes stay unfilled.
    pub fn paint(
        &self,
        painter: &mut Painter<'_>,
        bounds: Rect,
        color: Color,
        backing: Option<Color>,
    ) {
        let size = bounds.width().min(bounds.height());
        let transform = Affine::translate((
            bounds.x0 + (bounds.width() - size) / 2.0,
            bounds.y0 + (bounds.height() - size) / 2.0,
        )) * Affine::scale(size / 24.0);
        if let (Some(backing), Some(body)) = (backing, self.0.0.first()) {
            painter
                .fill(&body.path, backing)
                .transform(transform)
                .draw();
        }
        let stroke = Stroke::new(optical_stroke(size))
            .with_caps(Cap::Square)
            .with_join(Join::Miter)
            .with_miter_limit(2.0);
        for part in &self.0.0 {
            if let Some(rule) = part.fill {
                painter
                    .fill(&part.path, color)
                    .fill_rule(rule)
                    .transform(transform)
                    .draw();
            } else {
                painter
                    .stroke(&part.path, &stroke, color)
                    .transform(transform)
                    .draw();
            }
        }
    }
}
