use smithay::utils::{Logical, Point, Rectangle};

use crate::config::{MAX_ZOOM, MIN_ZOOM};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasPoint {
    pub x: i32,
    pub y: i32,
}

pub fn rect_contains(rect: Rectangle<i32, Logical>, point: Point<f64, Logical>) -> bool {
    let min_x = f64::from(rect.loc.x);
    let min_y = f64::from(rect.loc.y);
    let max_x = f64::from(rect.loc.x + rect.size.w);
    let max_y = f64::from(rect.loc.y + rect.size.h);

    point.x >= min_x && point.x < max_x && point.y >= min_y && point.y < max_y
}

pub fn canvas_to_screen(
    point: Point<f64, Logical>,
    viewport_offset: CanvasPoint,
    viewport_scale: f64,
) -> Point<f64, Logical> {
    (
        (point.x - f64::from(viewport_offset.x)) * viewport_scale,
        (point.y - f64::from(viewport_offset.y)) * viewport_scale,
    )
        .into()
}

pub fn screen_to_canvas(
    point: Point<f64, Logical>,
    viewport_offset: CanvasPoint,
    viewport_scale: f64,
) -> Point<f64, Logical> {
    (
        point.x / viewport_scale + f64::from(viewport_offset.x),
        point.y / viewport_scale + f64::from(viewport_offset.y),
    )
        .into()
}

pub fn zoom_around_screen_point(
    viewport_offset: CanvasPoint,
    viewport_scale: f64,
    center_screen: Point<f64, Logical>,
    multiplier: f64,
) -> (CanvasPoint, f64) {
    let new_scale = (viewport_scale * multiplier).clamp(MIN_ZOOM, MAX_ZOOM);
    if (new_scale - viewport_scale).abs() < f64::EPSILON {
        return (viewport_offset, viewport_scale);
    }

    let center_canvas = screen_to_canvas(center_screen, viewport_offset, viewport_scale);
    (
        CanvasPoint {
            x: (center_canvas.x - center_screen.x / new_scale).round() as i32,
            y: (center_canvas.y - center_screen.y / new_scale).round() as i32,
        },
        new_scale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_contains_includes_top_left_and_excludes_bottom_right() {
        let rect = Rectangle::new((10, 20).into(), (30, 40).into());

        assert!(rect_contains(rect, (10.0, 20.0).into()));
        assert!(rect_contains(rect, (39.99, 59.99).into()));
        assert!(!rect_contains(rect, (40.0, 60.0).into()));
        assert!(!rect_contains(rect, (9.99, 20.0).into()));
    }

    #[test]
    fn screen_canvas_transform_round_trips() {
        let offset = CanvasPoint { x: 100, y: -50 };
        let canvas_point = Point::<f64, Logical>::from((250.0, 150.0));
        let screen_point = canvas_to_screen(canvas_point, offset, 2.0);

        assert_eq!(screen_point, Point::<f64, Logical>::from((300.0, 400.0)));
        assert_eq!(screen_to_canvas(screen_point, offset, 2.0), canvas_point);
    }

    #[test]
    fn zoom_keeps_screen_center_on_same_canvas_point() {
        let offset = CanvasPoint { x: 10, y: 20 };
        let center = Point::<f64, Logical>::from((500.0, 300.0));
        let before = screen_to_canvas(center, offset, 1.0);
        let (new_offset, new_scale) = zoom_around_screen_point(offset, 1.0, center, 2.0);
        let after = screen_to_canvas(center, new_offset, new_scale);

        assert!((before.x - after.x).abs() <= 0.5);
        assert!((before.y - after.y).abs() <= 0.5);
        assert_eq!(new_scale, 2.0);
    }

    #[test]
    fn zoom_is_clamped() {
        let offset = CanvasPoint { x: 0, y: 0 };
        let center = Point::<f64, Logical>::from((0.0, 0.0));

        assert_eq!(
            zoom_around_screen_point(offset, 1.0, center, 100.0).1,
            MAX_ZOOM
        );
        assert_eq!(
            zoom_around_screen_point(offset, 1.0, center, 0.01).1,
            MIN_ZOOM
        );
    }
}
