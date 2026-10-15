use xilem::kurbo::{Point, Rect, Size};

use super::CanvasSnapshot;

const MAP_PADDING: f64 = 14.0;
const WORLD_GRID: f64 = 1024.0;

/// A stable, wide canvas neighborhood with room for nearby offscreen windows.
/// Unlike fitting only the visible output, this does not move during ordinary
/// panning. Distant windows and viewports expand it in coarse world-space steps.
#[derive(Clone, Debug)]
pub(super) struct Projection {
    pub(super) world: Rect,
    pub(super) scale: f64,
    origin: Point,
}

impl Projection {
    pub(super) fn new(snapshot: Option<&CanvasSnapshot>, size: Size) -> Self {
        let mut world = Rect::new(-4096.0, -2560.0, 4096.0, 2560.0);
        if let Some(snapshot) = snapshot {
            for window in &snapshot.windows {
                world = world.union(window.bounds);
            }
            for output in &snapshot.outputs {
                world = world.union(snapshot.output_bounds(output));
            }
        }
        world = Rect::new(
            (world.x0 / WORLD_GRID).floor() * WORLD_GRID,
            (world.y0 / WORLD_GRID).floor() * WORLD_GRID,
            (world.x1 / WORLD_GRID).ceil() * WORLD_GRID,
            (world.y1 / WORLD_GRID).ceil() * WORLD_GRID,
        );
        let scale = ((size.width - MAP_PADDING * 2.0).max(1.0) / world.width())
            .min((size.height - MAP_PADDING * 2.0).max(1.0) / world.height());
        let origin = Point::new(
            (size.width - world.width() * scale) / 2.0 - world.x0 * scale,
            (size.height - world.height() * scale) / 2.0 - world.y0 * scale,
        );
        Self {
            world,
            scale,
            origin,
        }
    }

    pub(super) fn point(&self, x: f64, y: f64) -> Point {
        Point::new(
            x * self.scale + self.origin.x,
            y * self.scale + self.origin.y,
        )
    }

    pub(super) fn rect(&self, rect: Rect) -> Rect {
        Rect::from_points(self.point(rect.x0, rect.y0), self.point(rect.x1, rect.y1))
    }
}
