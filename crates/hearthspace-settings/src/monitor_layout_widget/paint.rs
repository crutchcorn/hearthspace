//! Reference-inspired monitor tiles; all pixels stay within the preview surface.
use super::*;
use xilem::masonry::{
    TextAlign, TextAlignOptions,
    core::render_text,
    imaging::ClipRef,
    kurbo::{Affine, Circle, Stroke},
    parley::{
        FontFamilyName,
        style::{FontFamily, GenericFamily, StyleProperty},
    },
};

pub(super) fn preview(
    widget: &MonitorLayoutWidget,
    ctx: &mut PaintCtx<'_>,
    painter: &mut Painter<'_>,
) {
    let content = ctx.content_box();
    let frame = content.inset(-1.0).to_rounded_rect(components::RADIUS_TILE);
    painter.push_clip(ClipRef::fill(frame));
    painter.fill(frame, components::SURFACE_SUBTLE).draw();
    for x in (8..content.width() as usize).step_by(16) {
        for y in (8..content.height() as usize).step_by(16) {
            painter
                .fill(Circle::new((x as f64, y as f64), 0.7), components::BORDER)
                .draw();
        }
    }
    let metrics = widget.layout_metrics(content.size());
    // Selection paints last, so its accent edge remains visible at shared borders.
    for selected in [false, true] {
        for (index, monitor) in widget.monitors.iter().enumerate() {
            if (widget.selected_monitor_id.as_ref() == Some(&monitor.id)) != selected {
                continue;
            }
            let rect = metrics.rect(monitor);
            let tile = rect.to_rounded_rect(
                components::RADIUS_CONTROL
                    .min(rect.width() / 4.0)
                    .min(rect.height() / 4.0),
            );
            painter.fill(tile, components::CHROME_RAISED).draw();
            painter
                .stroke(
                    tile,
                    &Stroke::new(1.0),
                    if selected {
                        components::ACCENT
                    } else {
                        components::CHROME_BORDER
                    },
                )
                .draw();
            // Tiny previews still draw truthful geometry; labels never spill out of their tile.
            if rect.width() < 16.0 || rect.height() < 16.0 {
                continue;
            }
            let text = (index + 1).to_string();
            let (fcx, lcx) = ctx.text_contexts();
            let mut builder = lcx.ranged_builder(fcx, &text, 1.0, true);
            builder.push_default(StyleProperty::FontFamily(FontFamily::Single(
                FontFamilyName::Generic(GenericFamily::SystemUi),
            )));
            builder.push_default(StyleProperty::FontSize(
                24.0_f32.min((rect.height() * 0.5) as f32),
            ));
            let mut layout = builder.build(&text);
            layout.break_all_lines(None);
            layout.align(None, TextAlign::Start, TextAlignOptions::default());
            let origin = (
                rect.center().x - f64::from(layout.width()) / 2.0,
                rect.center().y - f64::from(layout.height()) / 2.0,
            );
            painter.push_clip(ClipRef::fill(tile));
            render_text(
                painter,
                Affine::translate(origin),
                &layout,
                &[if selected {
                    components::ACCENT_STRONG
                } else {
                    components::TEXT
                }
                .into()],
                true,
            );
            painter.pop_clip();
        }
    }
    painter.pop_clip();
    painter
        .stroke(
            frame,
            &Stroke::new(if ctx.is_focus_target() { 2.0 } else { 1.0 }),
            if ctx.is_focus_target() {
                components::FOCUS_RING
            } else {
                components::CHROME_BORDER
            },
        )
        .draw();
}
