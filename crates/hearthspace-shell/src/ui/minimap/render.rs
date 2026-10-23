use hearthspace_ui_components as components;
use xilem::{
    kurbo::{Line, Rect, Size, Stroke},
    masonry::imaging::Painter,
};

use super::{CanvasSnapshot, Projection};

pub(super) fn draw(
    painter: &mut Painter<'_>,
    size: Size,
    snapshot: Option<&CanvasSnapshot>,
    output: Option<&str>,
    projection: Option<&Projection>,
    palette: &components::Palette,
) {
    if size.width <= 28.0 || size.height <= 28.0 {
        return;
    }
    let fitted = Projection::new(snapshot, size);
    let map = projection.unwrap_or(&fitted);
    let grid_step = 2.0_f64.powf((32.0 / map.scale).log2().ceil());
    let start_x = (map.world.x0 / grid_step).ceil() * grid_step;
    let start_y = (map.world.y0 / grid_step).ceil() * grid_step;
    // At most about ten lines per direction, independent of canvas extent.
    for index in 0..=(map.world.width() / grid_step).ceil() as usize {
        let x = start_x + index as f64 * grid_step;
        if x <= map.world.x1 {
            painter
                .stroke(
                    Line::new(map.point(x, map.world.y0), map.point(x, map.world.y1)),
                    &Stroke::new(0.5),
                    palette.border,
                )
                .draw();
        }
    }
    for index in 0..=(map.world.height() / grid_step).ceil() as usize {
        let y = start_y + index as f64 * grid_step;
        if y <= map.world.y1 {
            painter
                .stroke(
                    Line::new(map.point(map.world.x0, y), map.point(map.world.x1, y)),
                    &Stroke::new(0.5),
                    palette.border,
                )
                .draw();
        }
    }
    let Some(snapshot) = snapshot else {
        return;
    };
    for window in &snapshot.windows {
        let projected = map.rect(window.bounds);
        let rect = Rect::from_center_size(
            projected.center(),
            Size::new(projected.width().max(3.0), projected.height().max(3.0)),
        )
        .to_rounded_rect(1.5);
        let fill = if window.focused {
            palette.accent_soft
        } else {
            palette.surface
        };
        let border = palette.text_muted;
        painter.fill(rect, fill).draw();
        painter.stroke(rect, &Stroke::new(1.0), border).draw();
    }
    let selected = output.or_else(|| snapshot.outputs.first().map(|output| output.name.as_str()));
    // Draw the selected viewport last so its frame stays visible on overlaps.
    for is_selected in [false, true] {
        for output in &snapshot.outputs {
            if (Some(output.name.as_str()) == selected) != is_selected {
                continue;
            }
            let rect = map.rect(snapshot.output_bounds(output));
            let (color, width) = if is_selected {
                (palette.accent, 2.0)
            } else {
                (palette.text_muted, 1.0)
            };
            if is_selected {
                painter.fill(rect, palette.accent.with_alpha(0.08)).draw();
            }
            painter.stroke(rect, &Stroke::new(width), color).draw();
        }
    }
}
