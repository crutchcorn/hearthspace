use hearthspace_ui_components::{
    Palette,
    settings_material::{dot_pattern, edge_light_with_palette, glass_edge_with_palette, shadow},
};
use xilem::{
    Color,
    kurbo::{Rect, Size, Stroke},
    masonry::{
        imaging::{ClipRef, Painter},
        peniko::Gradient,
    },
};

use super::{CanvasSnapshot, Projection, projection::MAP_BORDER};

#[derive(Clone, Copy, Default)]
pub(super) struct Interaction {
    pub emphasized: bool,
    pub focused: bool,
}

pub(super) fn draw(
    painter: &mut Painter<'_>,
    size: Size,
    snapshot: Option<&CanvasSnapshot>,
    output: Option<&str>,
    projection: Option<&Projection>,
    interaction: Interaction,
    palette: &Palette,
) {
    if size.width <= 28.0 || size.height <= 28.0 {
        return;
    }
    let rect = size.to_rect();
    let shape = rect.to_rounded_rect(10.0);
    if interaction.emphasized {
        shadow(
            painter,
            rect,
            10.0,
            palette.control_shadow,
            (0.0, 6.0),
            22.0,
            -5.0,
        );
    }
    painter.fill(shape, palette.surface_control).draw();
    let content = rect.inset(-MAP_BORDER);
    painter.push_clip(ClipRef::fill(content.to_rounded_rect(9.0)));
    dot_pattern(
        painter,
        content,
        12.0,
        0.65,
        0.8,
        palette.field_dot.multiply_alpha(0.65),
    );
    // These screen-space decorations stay stationary while the native
    // world projection expands to include distant windows and outputs.
    let mut x = content.x0;
    while x < content.x1 {
        painter
            .fill(
                Rect::new(x, content.y0, x + 1.0, content.y1),
                palette.surface_control,
            )
            .draw();
        x += 48.0;
    }
    let mut y = content.y0;
    while y < content.y1 {
        painter
            .fill(
                Rect::new(content.x0, y, content.x1, y + 1.0),
                palette.surface_control,
            )
            .draw();
        y += 48.0;
    }
    if interaction.emphasized {
        let glow = Gradient::new_radial(rect.center(), (size.width.max(size.height) / 2.0) as f32)
            .with_stops([(0.0, Color::TRANSPARENT), (1.0, palette.control_inset_glow)]);
        painter.fill(content, &glow).draw();
    }
    let fitted = Projection::new(snapshot, size);
    if let Some(snapshot) = snapshot {
        draw_contents(
            painter,
            snapshot,
            output,
            projection.unwrap_or(&fitted),
            palette,
        );
    }
    painter.pop_clip();
    let border = if interaction.emphasized {
        palette.accent_border
    } else {
        palette.chrome_border
    };
    painter
        .stroke(
            rect.inset(-0.5).to_rounded_rect(9.5),
            &Stroke::new(1.0),
            border,
        )
        .draw();
    glass_edge_with_palette(painter, rect, 10.0, false, palette);
    if interaction.focused {
        painter
            .stroke(
                rect.inflate(4.0, 4.0).to_rounded_rect(14.0),
                &Stroke::new(2.0),
                palette.text,
            )
            .draw();
    }
}

fn draw_contents(
    painter: &mut Painter<'_>,
    snapshot: &CanvasSnapshot,
    output: Option<&str>,
    map: &Projection,
    palette: &Palette,
) {
    for window in &snapshot.windows {
        let projected = map.rect(window.bounds);
        let rect = Rect::from_center_size(
            projected.center(),
            Size::new(projected.width().max(3.0), projected.height().max(3.0)),
        );
        // Focus remains in the accessible description, not a second selection
        // treatment competing with the viewport's accent outline.
        painter
            .fill(rect.to_rounded_rect(5.0), palette.chrome)
            .draw();
        edge_light_with_palette(painter, rect, 5.0, palette);
        painter
            .stroke(
                rect.inset(-0.5).to_rounded_rect(4.5),
                &Stroke::new(1.0),
                palette.muted,
            )
            .draw();
        glass_edge_with_palette(painter, rect, 5.0, false, palette);
    }
    let selected = output.or_else(|| snapshot.outputs.first().map(|output| output.name.as_str()));
    // Draw the selected viewport last so its frame stays visible on overlaps.
    for is_selected in [false, true] {
        for output in &snapshot.outputs {
            if (Some(output.name.as_str()) == selected) != is_selected {
                continue;
            }
            let rect = map.rect(snapshot.output_bounds(output));
            let shape = rect.to_rounded_rect(5.0);
            if is_selected {
                painter.fill(shape, palette.accent.with_alpha(0.09)).draw();
                // The one-pixel outer ring follows the two-pixel accent frame.
                painter
                    .stroke(
                        rect.inflate(0.5, 0.5).to_rounded_rect(5.5),
                        &Stroke::new(1.0),
                        palette.chrome_border,
                    )
                    .draw();
                painter
                    .stroke(
                        rect.inset(-1.0).to_rounded_rect(4.0),
                        &Stroke::new(2.0),
                        palette.accent,
                    )
                    .draw();
            } else {
                painter
                    .stroke(
                        rect.inset(-0.5).to_rounded_rect(4.5),
                        &Stroke::new(1.0),
                        palette.muted,
                    )
                    .draw();
            }
        }
    }
}
