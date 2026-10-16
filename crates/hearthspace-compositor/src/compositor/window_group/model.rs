//! Deterministic group selection, movement and animation geometry.
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

use smithay::utils::{Logical, Point, Rectangle, Size};

use crate::geometry::{CanvasPoint, ease_out_cubic, interpolate_canvas_point};

pub(super) const TOOLBAR_WIDTH: i32 = 360;
pub(super) const TOOLBAR_HEIGHT: i32 = 44;
pub(super) const ACTION_WIDTH: i32 = 44;
const FLIGHT_DURATION: Duration = Duration::from_millis(440);

pub(super) enum Gesture {
    Select {
        origin: Point<f64, Logical>,
        current: Point<f64, Logical>,
        base: BTreeSet<u64>,
    },
    Drag {
        origin: Point<f64, Logical>,
        scale: f64,
        positions: Vec<(u64, CanvasPoint)>,
    },
}

pub(super) struct Flight {
    pub(super) id: u64,
    pub(super) from: CanvasPoint,
    pub(super) to: CanvasPoint,
    pub(super) started: Instant,
}

impl Flight {
    pub(super) fn position(&self, now: Instant) -> (CanvasPoint, bool) {
        let progress = now.saturating_duration_since(self.started).as_secs_f64()
            / FLIGHT_DURATION.as_secs_f64();
        (
            interpolate_canvas_point(self.from, self.to, ease_out_cubic(progress)),
            progress >= 1.0,
        )
    }
}

pub(super) fn selection_rect(
    a: Point<f64, Logical>,
    b: Point<f64, Logical>,
) -> Rectangle<i32, Logical> {
    Rectangle::new(
        (a.x.min(b.x).floor() as i32, a.y.min(b.y).floor() as i32).into(),
        (
            (a.x - b.x).abs().ceil() as i32 + 1,
            (a.y - b.y).abs().ceil() as i32 + 1,
        )
            .into(),
    )
}

pub(super) fn moved_position(
    start: CanvasPoint,
    delta: Point<f64, Logical>,
    scale: f64,
) -> CanvasPoint {
    CanvasPoint {
        x: (f64::from(start.x) + delta.x / scale)
            .round()
            .clamp(-1_000_000.0, 1_000_000.0) as i32,
        y: (f64::from(start.y) + delta.y / scale)
            .round()
            .clamp(-1_000_000.0, 1_000_000.0) as i32,
    }
}

pub(super) fn toolbar_rect(
    bounds: Option<Rectangle<i32, Logical>>,
    output: Rectangle<i32, Logical>,
) -> Rectangle<i32, Logical> {
    let width = TOOLBAR_WIDTH.min(output.size.w.max(1));
    let height = TOOLBAR_HEIGHT.min(output.size.h.max(1));
    let anchor = bounds
        .map(|rect| {
            (
                rect.loc.x + (rect.size.w - width) / 2,
                rect.loc.y - height - 10,
            )
        })
        .unwrap_or((
            output.loc.x + (output.size.w - width) / 2,
            output.loc.y + 12,
        ));
    Rectangle::new(
        (
            anchor
                .0
                .clamp(output.loc.x, output.loc.x + output.size.w - width),
            anchor
                .1
                .clamp(output.loc.y, output.loc.y + output.size.h - height),
        )
            .into(),
        (width, height).into(),
    )
}

/// A diagonal fan keeps every gathered title bar reachable, even for oversized
/// clients and small/HiDPI outputs. Multiple rows wrap through the usable area.
pub(super) fn gather_target(
    index: usize,
    size: Size<i32, Logical>,
    output: Rectangle<i32, Logical>,
    scale: f64,
    offset: CanvasPoint,
) -> CanvasPoint {
    let margin_x = 16.min(output.size.w / 8);
    let margin_y = 64.min(output.size.h / 4);
    let span_x = (output.size.w - (f64::from(size.w) * scale).round() as i32)
        .max(output.size.w / 3)
        .min(output.size.w - margin_x - 1)
        .max(1);
    let span_y = (output.size.h - (f64::from(size.h) * scale).round() as i32)
        .max(output.size.h / 3)
        .min(output.size.h - margin_y - 1)
        .max(1);
    let x = output.loc.x + margin_x + (index as i64 * 42 % i64::from(span_x)) as i32;
    let y = output.loc.y + margin_y + (index as i64 * 36 % i64::from(span_y)) as i32;
    moved_position(offset, (f64::from(x), f64::from(y)).into(), scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toolbar_is_clamped_on_negative_secondary_output() {
        let output = Rectangle::new((-900, 32).into(), (900, 600).into());
        let bounds = Rectangle::new((-1300, -600).into(), (500, 400).into());
        assert!(output.contains_rect(toolbar_rect(Some(bounds), output)));
        let tiny = Rectangle::new((50, 60).into(), (180, 30).into());
        assert!(tiny.contains_rect(toolbar_rect(None, tiny)));
    }
    #[test]
    fn group_motion_preserves_offsets_at_zoom_and_clamps_coordinates() {
        let delta = (90.0, -36.0).into();
        let a = moved_position(CanvasPoint { x: 20, y: 60 }, delta, 1.5);
        let b = moved_position(CanvasPoint { x: 150, y: 200 }, delta, 1.5);
        assert_eq!((b.x - a.x, b.y - a.y), (130, 140));
        assert_eq!(a, CanvasPoint { x: 80, y: 36 });
        assert_eq!(
            moved_position(a, (1e15, -1e15).into(), 0.5),
            CanvasPoint {
                x: 1_000_000,
                y: -1_000_000
            }
        );
    }
    #[test]
    fn gather_fan_is_staggered_and_visible_after_canvas_transform() {
        let output = Rectangle::new((1200, 40).into(), (1000, 700).into());
        let offset = CanvasPoint { x: -1500, y: 1400 };
        for scale in [0.25, 1.0, 3.0] {
            let mut positions = BTreeSet::new();
            for index in 0..12 {
                let point = gather_target(index, (1400, 900).into(), output, scale, offset);
                assert!(positions.insert((point.x, point.y)));
                let screen = crate::geometry::canvas_to_screen(
                    (f64::from(point.x), f64::from(point.y)).into(),
                    offset,
                    scale,
                );
                assert!(output.contains(screen.to_i32_round()));
            }
        }
    }
    #[test]
    fn flights_start_at_original_position_and_finish_exactly() {
        let started = Instant::now();
        let flight = Flight {
            id: 1,
            from: CanvasPoint { x: -4000, y: 5000 },
            to: CanvasPoint { x: 100, y: 200 },
            started,
        };
        assert_eq!(flight.position(started), (flight.from, false));
        assert_eq!(
            flight.position(started + FLIGHT_DURATION),
            (flight.to, true)
        );
        let halfway = flight.position(started + FLIGHT_DURATION / 2);
        assert!(halfway.0.x > -1000 && !halfway.1);
    }

    #[test]
    fn gathering_small_clients_keeps_origins_inside_small_outputs() {
        for output in [
            Rectangle::new((-500, 40).into(), (500, 250).into()),
            Rectangle::new((0, 48).into(), (100, 80).into()),
        ] {
            for index in 0..256 {
                let position = gather_target(
                    index,
                    (1, 1).into(),
                    output,
                    1.0,
                    CanvasPoint { x: 0, y: 0 },
                );
                assert!(output.contains(Point::from((position.x, position.y))));
            }
        }
    }
}
