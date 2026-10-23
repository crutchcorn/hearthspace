//! Browser-equivalent cubic easing and clipping of invisible preview travel.
use super::Rect;

/// The scene's `cubic-bezier(.2,.7,.1,1)` transform easing.
pub fn ease(progress: f64) -> f64 {
    cubic(progress, 0.2, 0.7, 0.1, 1.0)
}

/// The selected-window reveal's `cubic-bezier(.22,.61,.36,1)` easing.
pub fn reveal_ease(progress: f64) -> f64 {
    cubic(progress, 0.22, 0.61, 0.36, 1.0)
}

/// The scene's 220ms CSS `ease` opacity curve.
pub fn opacity_ease(progress: f64) -> f64 {
    cubic(progress, 0.25, 0.1, 0.25, 1.0)
}

/// CSS `ease-out`, used by frame guide entry.
pub fn ease_out(progress: f64) -> f64 {
    cubic(progress, 0.0, 0.0, 0.58, 1.0)
}

fn cubic(progress: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }
    let sample = |t: f64, a: f64, b: f64| {
        3.0 * (1.0 - t).powi(2) * t * a + 3.0 * (1.0 - t) * t * t * b + t.powi(3)
    };
    // Invert x, rather than treating time as the Bezier parameter. Bisection is
    // stable even near the flat end of a curve and needs no per-frame allocation.
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if sample(mid, x1, x2) < progress {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    sample((lo + hi) / 2.0, y1, y2)
}

pub fn interpolate(start: Rect, end: Rect, progress: f64) -> Rect {
    let lerp = |a: f64, b: f64| a + (b - a) * progress;
    Rect {
        x: lerp(start.x, end.x),
        y: lerp(start.y, end.y),
        width: lerp(start.width, end.width),
        height: lerp(start.height, end.height),
    }
}

/// Trim only the invisible part of a rectangle's path, preserving its path
/// through the viewport and the reference's 48px edge clearance.
pub fn visible_travel(start: Rect, end: Rect, view: Rect) -> Option<(Rect, Rect)> {
    let distances = |rect: Rect| {
        [
            rect.x + rect.width - view.x + 48.0,
            view.x + view.width + 48.0 - rect.x,
            rect.y + rect.height - view.y + 48.0,
            view.y + view.height + 48.0 - rect.y,
        ]
    };
    let (mut enter, mut exit) = (0.0_f64, 1.0_f64);
    for (distance, end_distance) in distances(start).into_iter().zip(distances(end)) {
        let change = end_distance - distance;
        if change == 0.0 {
            if distance < 0.0 {
                return None;
            }
        } else if change > 0.0 {
            enter = enter.max(-distance / change);
        } else {
            exit = exit.min(-distance / change);
        }
        if enter > exit {
            return None;
        }
    }
    Some((
        interpolate(start, end, enter),
        interpolate(start, end, exit),
    ))
}
