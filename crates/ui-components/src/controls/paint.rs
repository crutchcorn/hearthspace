use super::{ControlKind, ControlWidget};
use crate::{
    Palette,
    icons::{Icon, IconGlyph},
    settings_material::{glass_edge_with_palette, shadow},
};
use xilem::masonry::{
    core::PaintCtx,
    imaging::Painter,
    kurbo::{Rect, Stroke},
    peniko::Gradient,
};

pub(super) fn control<A>(
    w: &ControlWidget<A>,
    ctx: &mut PaintCtx<'_>,
    p: Palette,
    paint: &mut Painter<'_>,
) {
    let r = ctx.content_box();
    let alpha = if ctx.is_disabled() { 0.5 } else { 1. };
    let focused = w.focus.visible() && ctx.is_focus_target() && ctx.is_window_focused();
    if w.kind == ControlKind::Switch {
        super::toggle::paint::paint(
            p,
            paint,
            r.x0,
            r.y0,
            w.value != 0.,
            ctx.is_disabled(),
            focused,
        );
        return;
    }
    let fill = if p.dark {
        Gradient::new_linear((0., r.y0), (0., r.y1)).with_stops([
            p.dark_control_fill().multiply_alpha(alpha),
            p.dark_control_fill().multiply_alpha(alpha),
        ])
    } else {
        Gradient::new_linear((r.x0, r.y0), (r.x1, r.y1)).with_stops([
            p.accent_bright.multiply_alpha(alpha),
            p.accent.multiply_alpha(alpha),
        ])
    };
    match w.kind {
        ControlKind::Switch => unreachable!("switch track is painted above"),
        ControlKind::Checkbox => {
            let shape = r.inset(-0.5).to_rounded_rect(5.);
            if w.value != 0. {
                paint.fill(shape, &fill).draw();
            } else {
                paint
                    .fill(shape, p.surface_control.multiply_alpha(alpha))
                    .draw();
            }
            paint
                .stroke(
                    shape,
                    &Stroke::new(1.),
                    if w.value != 0. || (ctx.is_hovered() && !ctx.is_disabled()) {
                        p.accent_border
                    } else {
                        p.checkbox_border()
                    }
                    .multiply_alpha(alpha),
                )
                .draw();
            if w.value != 0. {
                IconGlyph::new(Icon::Check, false).paint(
                    paint,
                    Rect::new(r.x0 + 2., r.y0 + 2., r.x0 + 18., r.y0 + 18.),
                    if p.dark {
                        p.control_thumb
                    } else {
                        p.accent_ink
                    }
                    .multiply_alpha(alpha),
                    None,
                );
            }
        }
        ControlKind::Meter | ControlKind::Progress | ControlKind::Slider => {
            let slider = w.kind == ControlKind::Slider;
            let left = r.x0;
            let right = r.x1;
            let y = r.center().y;
            let h = if slider { 10. } else { 8. };
            let rail = Rect::new(left, y - h / 2., right, y + h / 2.).to_rounded_rect(h / 2.);
            paint
                .fill(rail, p.control_track.multiply_alpha(alpha))
                .draw();
            let fraction = (w.value - w.minimum) / (100. - w.minimum);
            let end = left + (right - left) * fraction;
            if end > left {
                let fill_shape =
                    Rect::new(left, y - h / 2., end, y + h / 2.).to_rounded_rect(h / 2.);
                let gradient = if slider {
                    Gradient::new_linear((left, y - h / 2.), (left, y + h / 2.)).with_stops([
                        if p.dark { p.accent } else { p.accent_bright }.multiply_alpha(alpha),
                        if p.dark { p.accent_base } else { p.accent }.multiply_alpha(alpha),
                    ])
                } else {
                    Gradient::new_linear((left, 0.), (end, 0.)).with_stops([
                        p.accent_light.multiply_alpha(alpha),
                        p.accent.multiply_alpha(alpha),
                    ])
                };
                paint.fill(fill_shape, &gradient).draw();
            }
            if slider {
                let thumb = Rect::new(end - 10., y - 10., end + 10., y + 10.);
                shadow(
                    paint,
                    thumb,
                    10.,
                    p.border.multiply_alpha(alpha),
                    (0., 1.),
                    3.,
                    0.,
                );
                paint
                    .fill(
                        thumb.to_rounded_rect(10.),
                        p.control_thumb.multiply_alpha(alpha),
                    )
                    .draw();
                paint
                    .stroke(
                        thumb.inset(-0.5).to_rounded_rect(10.),
                        &Stroke::new(1.),
                        if ctx.is_hovered() || ctx.is_active() {
                            p.accent_border
                        } else {
                            p.border
                        }
                        .multiply_alpha(alpha),
                    )
                    .draw();
                glass_edge_with_palette(paint, thumb, 10., true, &p);
            }
        }
    }
    if focused && !ctx.is_disabled() {
        let (bounds, radius) = if w.kind == ControlKind::Slider {
            let x = r.x0 + r.width() * (w.value - w.minimum) / (100. - w.minimum);
            (
                Rect::new(x - 15., r.center().y - 15., x + 15., r.center().y + 15.),
                15.,
            )
        } else {
            (
                r.inflate(3., 3.),
                if w.kind == ControlKind::Switch {
                    15.
                } else {
                    8.
                },
            )
        };
        paint
            .stroke(
                bounds.to_rounded_rect(radius),
                &Stroke::new(2.),
                p.focus_ring,
            )
            .draw();
    }
}
