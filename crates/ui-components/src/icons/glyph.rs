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

    /// Export the same paths for clients using another native renderer (the
    /// session locker uses GTK's secure password editor and Wayland lock role).
    pub fn svg(&self, size: f64, color: Color) -> String {
        use std::fmt::Write;
        let rgba = color.to_rgba8();
        let color = format!("#{:02x}{:02x}{:02x}", rgba.r, rgba.g, rgba.b);
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{size}\" height=\"{size}\" viewBox=\"0 0 24 24\" stroke-linecap=\"square\" stroke-linejoin=\"miter\" stroke-miterlimit=\"2\" opacity=\"{}\">",
            f64::from(rgba.a) / 255.0
        );
        for part in &self.0.0 {
            let path = part.path.to_svg();
            if let Some(rule) = part.fill {
                let rule = if rule == xilem::masonry::peniko::Fill::EvenOdd {
                    "evenodd"
                } else {
                    "nonzero"
                };
                write!(
                    svg,
                    "<path d=\"{path}\" fill=\"{color}\" fill-rule=\"{rule}\"/>"
                )
                .unwrap();
            } else {
                write!(
                    svg,
                    "<path d=\"{path}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\"/>",
                    optical_stroke(size)
                )
                .unwrap();
            }
        }
        svg.push_str("</svg>");
        svg
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
