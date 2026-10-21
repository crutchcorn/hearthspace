//! One canvas-space grid for the wallpaper, desktop items, and window bounds.
pub const CANVAS_GRID_SIZE: i32 = 28;
pub const CANVAS_GRID_OFFSET: i32 = CANVAS_GRID_SIZE / 2;

/// Nearest reachable dot. Constraints win when their interval contains no dot.
pub fn snap_coordinate(value: f64, min: f64, max: f64) -> f64 {
    let step = f64::from(CANVAS_GRID_SIZE);
    let offset = f64::from(CANVAS_GRID_OFFSET);
    let first = ((min - offset) / step).ceil();
    let last = ((max - offset) / step).floor();
    if first > last {
        return value.clamp(min, max);
    }
    ((value - offset) / step).round().clamp(first, last) * step + offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_dot_respects_negative_coordinates_and_reachable_bounds() {
        assert_eq!(snap_coordinate(125.0, -1e6, 1e6), 126.0);
        assert_eq!(snap_coordinate(-39.0, -1e6, 1e6), -42.0);
        assert_eq!(snap_coordinate(125.0, 128.0, 180.0), 154.0);
        assert_eq!(snap_coordinate(180.0, 128.0, 150.0), 150.0);
        assert_eq!(snap_coordinate(140.0, 140.0, 140.0), 140.0);
    }
}
