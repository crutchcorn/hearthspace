//! Painting for the shared gesture collector and external stroke preview.
use super::*;

impl DrawingPad {
    pub(super) fn paint_background(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        xilem::masonry::core::pre_paint(ctx, props, painter);
        let palette = *props.get::<components::Palette>(ctx.property_cache());
        if self.capture_locally
            && (ctx.is_hovered() || ctx.is_focus_target() || self.stroke.is_some())
        {
            for (color, offset, blur, spread) in [
                (
                    palette
                        .accent
                        .with_alpha(if palette.dark { 0.14 } else { 0.18 }),
                    (0.0, 28.0),
                    88.0,
                    -18.0,
                ),
                (palette.glow_shadow, (0.0, 8.0), 32.0, -12.0),
            ] {
                components::settings_material::shadow(
                    painter,
                    self.size.to_rect(),
                    14.0,
                    color,
                    offset,
                    blur,
                    spread,
                );
            }
        }
    }

    pub(super) fn paint_content(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let palette = *props.get::<components::Palette>(ctx.property_cache());
        let bounds = self.size.to_rect().inset(-1.0).to_rounded_rect(14.0);
        if self.capture_locally {
            let rect = self.size.to_rect();
            let border = rect.inset(-0.5).to_rounded_rect(13.5);
            let active = ctx.is_hovered() || ctx.is_focus_target() || self.stroke.is_some();
            painter
                .fill(rect.to_rounded_rect(14.0), palette.chrome)
                .draw();
            components::settings_material::edge_light_with_palette(painter, rect, 14.0, &palette);
            painter
                .stroke(
                    border,
                    &Stroke::new(1.0),
                    if active {
                        palette.accent_border
                    } else {
                        palette.chrome_border
                    },
                )
                .draw();
            components::settings_material::glass_edge_with_palette(
                painter, rect, 14.0, active, &palette,
            );
        } else if ctx.is_focus_target() {
            painter
                .stroke(bounds, &Stroke::new(1.0), palette.focus_ring_on_glass)
                .draw();
        }
        let mut path = BezPath::new();
        for (index, point) in self.points().iter().enumerate() {
            if index == 0 {
                path.move_to(Point::new(point[0], point[1]));
            } else {
                path.line_to(Point::new(point[0], point[1]));
            }
        }
        painter
            .stroke(
                path,
                &Stroke::new(if self.capture_locally { 2.5 } else { 3.0 })
                    .with_caps(Cap::Round)
                    .with_join(Join::Round),
                if self.capture_locally {
                    palette.accent_decorative
                } else {
                    palette.accent
                },
            )
            .draw();
        if !self.capture_locally
            && let Some(point) = self.points().first()
        {
            painter
                .fill(
                    Circle::new(Point::new(point[0], point[1]), 3.5),
                    palette.accent,
                )
                .draw();
        }
    }
}
