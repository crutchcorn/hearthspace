use smithay::utils::{Logical, Point};

use crate::{
    config::{MAX_ZOOM, MIN_ZOOM},
    geometry::CanvasPoint,
};

pub(super) fn finite_point(point: Point<f64, Logical>) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

/// The canvas point initially below `anchor` follows the fingers to
/// `anchor + translation`. Scale is clamped before computing that transform;
/// neither translation nor an intermediate zoom offset is rounded.
pub(super) fn gesture_viewport(
    offset: CanvasPoint,
    scale: f64,
    anchor: Point<f64, Logical>,
    factor: f64,
    translation: Point<f64, Logical>,
) -> Option<(CanvasPoint, f64)> {
    if !factor.is_finite() || factor <= 0.0 || !finite_point(translation) {
        return None;
    }
    let next_scale = (scale * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    let inverse_change = scale.recip() - next_scale.recip();
    let next = Point::from((
        f64::from(offset.x) + anchor.x * inverse_change - translation.x / next_scale,
        f64::from(offset.y) + anchor.y * inverse_change - translation.y / next_scale,
    ));
    finite_point(next).then_some((
        CanvasPoint {
            x: next.x.round() as i32,
            y: next.y.round() as i32,
        },
        next_scale,
    ))
}
