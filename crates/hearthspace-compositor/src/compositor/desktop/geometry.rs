use crate::geometry::{CanvasPoint, grid::snap_coordinate};
use smithay::utils::{Logical, Point, Rectangle};

pub(super) fn rectangle(a: Point<f64, Logical>, b: Point<f64, Logical>) -> Rectangle<f64, Logical> {
    Rectangle::new(
        (a.x.min(b.x), a.y.min(b.y)).into(),
        ((a.x - b.x).abs(), (a.y - b.y).abs()).into(),
    )
}

pub(super) fn moved_position(
    origin: CanvasPoint,
    delta: Point<f64, Logical>,
    snap: bool,
) -> CanvasPoint {
    let align = |value: f64| {
        if snap {
            snap_coordinate(value, -1_000_000.0, 1_000_000.0)
        } else {
            value.round()
        }
    };
    CanvasPoint {
        x: align(f64::from(origin.x) + delta.x).clamp(-1_000_000.0, 1_000_000.0) as i32,
        y: align(f64::from(origin.y) + delta.y).clamp(-1_000_000.0, 1_000_000.0) as i32,
    }
}

pub(super) fn edge_speed(
    pointer: Point<f64, Logical>,
    output: Rectangle<i32, Logical>,
) -> Point<f64, Logical> {
    let speed = |p: f64, low: f64, high: f64| {
        if p < low + 32.0 {
            -420.0 * ((low + 32.0 - p) / 32.0).clamp(0.0, 1.0)
        } else if p > high - 32.0 {
            420.0 * ((p - high + 32.0) / 32.0).clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    (
        speed(
            pointer.x,
            f64::from(output.loc.x),
            f64::from(output.loc.x + output.size.w),
        ),
        speed(
            pointer.y,
            f64::from(output.loc.y),
            f64::from(output.loc.y + output.size.h),
        ),
    )
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_preserves_spacing_and_snaps_the_grabbed_anchor() {
        let origin = CanvasPoint { x: 31, y: -11 };
        let moved = moved_position(origin, (16.0, 5.0).into(), true);
        assert_eq!(moved, CanvasPoint { x: 42, y: -14 });
        let delta = (f64::from(moved.x - origin.x), f64::from(moved.y - origin.y)).into();
        assert_eq!(
            moved_position(CanvasPoint { x: 135, y: 93 }, delta, false),
            CanvasPoint { x: 146, y: 90 }
        );
        assert_eq!(
            moved_position(origin, (f64::MAX, -f64::MAX).into(), false),
            CanvasPoint {
                x: 1_000_000,
                y: -1_000_000
            }
        );
    }
    #[test]
    fn marquee_handles_reverse_drag_and_edge_pan_is_output_local() {
        assert_eq!(
            rectangle((10.0, 40.0).into(), (-5.0, 20.0).into()),
            Rectangle::new((-5.0, 20.0).into(), (15.0, 20.0).into())
        );
        let output = Rectangle::new((100, 50).into(), (800, 600).into());
        assert_eq!(edge_speed((500.0, 350.0).into(), output), (0.0, 0.0).into());
        assert_eq!(
            edge_speed((100.0, 650.0).into(), output),
            (-420.0, 420.0).into()
        );
    }
}
