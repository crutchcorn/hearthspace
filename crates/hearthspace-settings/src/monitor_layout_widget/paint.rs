//! Reference-inspired monitor tiles; all pixels stay within the preview surface.
use super::*;
use xilem::masonry::{
    TextAlign, TextAlignOptions,
    core::render_text,
    imaging::ClipRef,
    kurbo::{Affine, Stroke},
    parley::{
        FontFamilyName,
        style::{FontFamily, StyleProperty},
    },
};

pub(super) fn preview(
    palette: components::Palette,
    widget: &MonitorLayoutWidget,
    ctx: &mut PaintCtx<'_>,
    painter: &mut Painter<'_>,
) {
    let content = ctx.content_box();
    let frame = content.inset(-0.5).to_rounded_rect(components::RADIUS_TILE);
    painter.push_clip(ClipRef::fill(frame));
    painter.fill(frame, palette.surface_subtle).draw();
    components::settings_material::dot_pattern(
        painter,
        content.inset(-1.0),
        16.0,
        0.7,
        1.0,
        palette.field_dot,
    );
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
            let hovered = widget.hovered_monitor_id.as_ref() == Some(&monitor.id);
            let emphasized = selected
                || hovered
                || widget
                    .drag
                    .as_ref()
                    .is_some_and(|drag| drag.id == monitor.id);
            let radius = tile.radii().top_left;
            if emphasized {
                components::settings_material::shadow(
                    painter,
                    rect,
                    radius,
                    palette.accent.with_alpha(0.18),
                    (0.0, 28.0),
                    88.0,
                    -18.0,
                );
                components::settings_material::shadow(
                    painter,
                    rect,
                    radius,
                    palette.glow_shadow,
                    (0.0, 8.0),
                    32.0,
                    -12.0,
                );
            } else {
                components::settings_material::shadow(
                    painter,
                    rect,
                    radius,
                    palette.window_shadow,
                    (0.0, 24.0),
                    64.0,
                    -20.0,
                );
                components::settings_material::shadow(
                    painter,
                    rect,
                    radius,
                    palette.accent.with_alpha(0.15),
                    (0.0, 6.0),
                    20.0,
                    -8.0,
                );
            }
            // Precompose the blurred field so its dots do not show through the tile.
            painter.fill(tile, palette.popup_backing).draw();
            painter.fill(tile, palette.chrome_raised).draw();
            components::settings_material::edge_light_with_palette(painter, rect, radius, &palette);
            if hovered {
                painter.fill(tile, palette.raised_button_hover).draw();
            }
            painter
                .stroke(
                    tile,
                    &Stroke::new(1.0),
                    if selected {
                        palette.accent
                    } else if hovered {
                        palette.accent_border
                    } else {
                        palette.chrome_border
                    },
                )
                .draw();
            components::settings_material::glass_edge_with_palette(
                painter, rect, radius, emphasized, &palette,
            );
            if selected && ctx.is_focus_target() && widget.focus.visible() {
                painter
                    .stroke(
                        rect.inflate(3.0, 3.0).to_rounded_rect(radius + 3.0),
                        &Stroke::new(2.0),
                        palette.focus_ring,
                    )
                    .draw();
            }
            // Tiny previews still draw truthful geometry; labels never spill out of their tile.
            if rect.width() < 16.0 || rect.height() < 16.0 {
                continue;
            }
            let text = (index + 1).to_string();
            let (fcx, lcx) = ctx.text_contexts();
            let mut builder = lcx.ranged_builder(fcx, &text, 1.0, true);
            builder.push_default(StyleProperty::FontFamily(FontFamily::Single(
                FontFamilyName::Named(crate::typography::FONT.into()),
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
                    palette.accent_decorative
                } else {
                    palette.text
                }
                .into()],
                true,
            );
            painter.pop_clip();
        }
    }
    components::settings_material::glass_edge_with_palette(
        painter,
        content,
        components::RADIUS_TILE,
        false,
        &palette,
    );
    painter.pop_clip();
    painter
        .stroke(frame, &Stroke::new(1.0), palette.border)
        .draw();
}
