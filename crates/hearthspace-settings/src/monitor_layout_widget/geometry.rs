//! One reversible projection for preview painting, hit testing and captured drags.
use super::*;
use xilem::masonry::kurbo::{Rect, Vec2};

// Leave only the preview outline's inner stroke, not an invisible drag margin.
const EDITOR_PADDING: f64 = 2.0;
const MONITOR_SNAP: i32 = 10;
const MONITOR_LAYOUT_CANVAS_MULTIPLIER: f64 = 1.8;

#[derive(Clone, Copy, Debug)]
pub(super) struct LayoutMetrics {
    pub canvas: Rect,
    pub scale: f64,
    offset: Vec2,
    viewport: Rect,
}

impl LayoutMetrics {
    pub fn new(monitors: &[MonitorConfig], size: Size) -> Self {
        let canvas_size = monitor_canvas_size(monitors);
        let origin = monitor_canvas_origin(monitors, canvas_size);
        Self::from_canvas(Rect::from_origin_size(origin, canvas_size), size)
    }

    pub fn from_canvas(mut canvas: Rect, size: Size) -> Self {
        let padding = EDITOR_PADDING
            .min(size.width / 4.0)
            .min(size.height / 4.0)
            .max(0.0);
        let viewport = Rect::new(
            padding,
            padding,
            (size.width - padding).max(padding),
            (size.height - padding).max(padding),
        );
        let scale = (viewport.width() / canvas.width())
            .min(viewport.height() / canvas.height())
            .clamp(f64::EPSILON, 1.0);
        // Uniform scaling can leave letterboxing on either axis. That space is
        // visible inside the board, so include it in the draggable world area.
        // Preserve an already fitted canvas to avoid floating-point drift when
        // rebuilding the frozen projection during a drag.
        let center = canvas.center();
        if canvas.width() * scale < viewport.width() - 1e-6 {
            let half_width = viewport.width() / scale / 2.0;
            canvas.x0 = center.x - half_width;
            canvas.x1 = center.x + half_width;
        }
        if canvas.height() * scale < viewport.height() - 1e-6 {
            let half_height = viewport.height() / scale / 2.0;
            canvas.y0 = center.y - half_height;
            canvas.y1 = center.y + half_height;
        }
        Self {
            canvas,
            scale,
            offset: viewport.center().to_vec2() - canvas.center().to_vec2() * scale,
            viewport,
        }
    }

    pub fn rect(self, monitor: &MonitorConfig) -> Rect {
        Rect::from_origin_size(
            (
                f64::from(monitor.x) * self.scale + self.offset.x,
                f64::from(monitor.y) * self.scale + self.offset.y,
            ),
            (
                f64::from(monitor_logical_width(monitor)) * self.scale,
                f64::from(monitor_logical_height(monitor)) * self.scale,
            ),
        )
    }

    pub fn world_point(self, point: Point) -> Point {
        ((point.to_vec2() - self.offset) / self.scale).to_point()
    }

    pub fn contains(self, point: Point) -> bool {
        self.viewport.contains(point)
    }

    pub fn bounded_position(
        self,
        monitors: &[MonitorConfig],
        id: &str,
        x: i32,
        y: i32,
    ) -> (i32, i32) {
        let Some(monitor) = monitors.iter().find(|monitor| monitor.id == id) else {
            return (x, y);
        };
        let width = monitor_logical_width(monitor);
        let height = monitor_logical_height(monitor);
        let min_x = self.canvas.x0.ceil() as i32;
        let min_y = self.canvas.y0.ceil() as i32;
        let max_x = (self.canvas.x1 - f64::from(width)).floor() as i32;
        let max_y = (self.canvas.y1 - f64::from(height)).floor() as i32;
        let x = snap_position(x).clamp(min_x, max_x.max(min_x));
        let y = snap_position(y).clamp(min_y, max_y.max(min_y));
        (x, y)
    }
}

pub(super) fn monitor_canvas_size(monitors: &[MonitorConfig]) -> Size {
    let (width, height) = if let Some((x0, y0, x1, y1)) = monitor_arrangement_bounds(monitors) {
        (
            (x1 - x0).max(
                monitors
                    .iter()
                    .map(|m| i64::from(monitor_logical_width(m)))
                    .sum(),
            ),
            (y1 - y0).max(
                monitors
                    .iter()
                    .map(|m| i64::from(monitor_logical_height(m)))
                    .sum(),
            ),
        )
    } else {
        (
            i64::from(EMPTY_MONITOR_CANVAS_WIDTH),
            i64::from(EMPTY_MONITOR_CANVAS_HEIGHT),
        )
    };
    Size::new(
        width.max(1) as f64 * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
        height.max(1) as f64 * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
    )
}

pub(super) fn monitor_canvas_origin(monitors: &[MonitorConfig], canvas_size: Size) -> (f64, f64) {
    let (cx, cy) = monitor_arrangement_bounds(monitors).map_or((0.0, 0.0), |(x0, y0, x1, y1)| {
        ((x0 as f64 + x1 as f64) / 2.0, (y0 as f64 + y1 as f64) / 2.0)
    });
    (cx - canvas_size.width / 2.0, cy - canvas_size.height / 2.0)
}

pub(super) fn set_monitor_position(monitors: &mut [MonitorConfig], id: &str, x: i32, y: i32) {
    if let Some(monitor) = monitors.iter_mut().find(|monitor| monitor.id == id) {
        monitor.x = x;
        monitor.y = y;
    }
}

pub(super) fn normalize_monitors_near_origin(monitors: &mut [MonitorConfig]) {
    let Some((min_x, min_y, max_x, max_y)) = monitor_arrangement_bounds(monitors) else {
        return;
    };
    // Choose a shared translation that keeps every i32 origin representable.
    // Clamping monitors independently would alter their real relative layout.
    let translation = |preferred: i64, positions: Vec<i32>| {
        let minimum = positions
            .iter()
            .map(|&position| i64::from(position) - i64::from(i32::MAX))
            .max()
            .unwrap();
        let maximum = positions
            .iter()
            .map(|&position| i64::from(position) - i64::from(i32::MIN))
            .min()
            .unwrap();
        preferred.clamp(minimum, maximum)
    };
    let dx = translation((min_x + max_x) / 2, monitors.iter().map(|m| m.x).collect());
    let dy = translation((min_y + max_y) / 2, monitors.iter().map(|m| m.y).collect());
    for monitor in monitors {
        monitor.x = (i64::from(monitor.x) - dx) as i32;
        monitor.y = (i64::from(monitor.y) - dy) as i32;
    }
}

pub(super) fn monitor_arrangement_bounds(
    monitors: &[MonitorConfig],
) -> Option<(i64, i64, i64, i64)> {
    let first = monitors.first()?;
    let mut bounds = (
        i64::from(first.x),
        i64::from(first.y),
        i64::from(first.x) + i64::from(monitor_logical_width(first)),
        i64::from(first.y) + i64::from(monitor_logical_height(first)),
    );
    for monitor in monitors.iter().skip(1) {
        bounds.0 = bounds.0.min(i64::from(monitor.x));
        bounds.1 = bounds.1.min(i64::from(monitor.y));
        bounds.2 = bounds
            .2
            .max(i64::from(monitor.x) + i64::from(monitor_logical_width(monitor)));
        bounds.3 = bounds
            .3
            .max(i64::from(monitor.y) + i64::from(monitor_logical_height(monitor)));
    }
    Some(bounds)
}

pub(super) fn snap_position(value: i32) -> i32 {
    let value = i64::from(value);
    let half = i64::from(MONITOR_SNAP) / 2;
    let shifted = if value >= 0 {
        value + half
    } else {
        value - half
    };
    ((shifted / i64::from(MONITOR_SNAP)) * i64::from(MONITOR_SNAP))
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

pub(super) fn snap_monitor_to_neighbor(
    monitors: &[MonitorConfig],
    id: &str,
    x: i32,
    y: i32,
) -> (i32, i32) {
    let Some(monitor) = monitors.iter().find(|monitor| monitor.id == id) else {
        return (x, y);
    };
    monitors
        .iter()
        .filter(|other| other.id != id)
        .flat_map(|other| bordered_positions_for(monitor, other, x, y))
        .flatten()
        .min_by_key(|&(candidate_x, candidate_y)| {
            let dx = i128::from(candidate_x) - i128::from(x);
            let dy = i128::from(candidate_y) - i128::from(y);
            dx * dx + dy * dy
        })
        .unwrap_or((x, y))
}

pub(super) fn bordered_positions_for(
    monitor: &MonitorConfig,
    other: &MonitorConfig,
    x: i32,
    y: i32,
) -> [Option<(i32, i32)>; 4] {
    let width = i64::from(monitor_logical_width(monitor));
    let height = i64::from(monitor_logical_height(monitor));
    let other_width = i64::from(monitor_logical_width(other));
    let other_height = i64::from(monitor_logical_height(other));
    let ox = i64::from(other.x);
    let oy = i64::from(other.y);
    let x = clamp_overlap(i64::from(x), width, ox, other_width);
    let y = clamp_overlap(i64::from(y), height, oy, other_height);
    let position = |x, y| Some((i32::try_from(x).ok()?, i32::try_from(y).ok()?));
    [
        position(ox - width, y),
        position(ox + other_width, y),
        position(x, oy - height),
        position(x, oy + other_height),
    ]
}

fn clamp_overlap(position: i64, size: i64, other_position: i64, other_size: i64) -> i64 {
    position.clamp(
        other_position - size.max(1) + 1,
        other_position + other_size.max(1) - 1,
    )
}
