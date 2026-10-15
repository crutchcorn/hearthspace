use smithay::utils::{Logical, Point, Rectangle};

pub(super) fn primary_output_center(
    output_rects: &[Rectangle<i32, Logical>],
) -> Option<Point<f64, Logical>> {
    let rect = output_rects.first()?;
    Some(Point::from((
        f64::from(rect.loc.x) + f64::from(rect.size.w.max(1)) / 2.0,
        f64::from(rect.loc.y) + f64::from(rect.size.h.max(1)) / 2.0,
    )))
}

pub(super) fn clamp_point_to_outputs(
    location: Point<f64, Logical>,
    output_rects: &[Rectangle<i32, Logical>],
) -> Point<f64, Logical> {
    output_rects
        .iter()
        .map(|rect| clamp_point_to_rect(location, *rect))
        .min_by(|a, b| {
            squared_distance(location, *a)
                .partial_cmp(&squared_distance(location, *b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(location)
}

pub(super) struct PointerConstraint {
    pub(super) visible: Point<f64, Logical>,
    pub(super) raw: Point<f64, Logical>,
}

pub(super) fn constrain_pointer_to_outputs(
    previous: Point<f64, Logical>,
    location: Point<f64, Logical>,
    output_rects: &[Rectangle<i32, Logical>],
) -> PointerConstraint {
    let previous_rect = rect_containing_point(previous, output_rects)
        .or_else(|| nearest_rect_to_point(previous, output_rects));

    if rect_containing_point(location, output_rects).is_some() {
        return PointerConstraint {
            visible: location,
            raw: location,
        };
    }

    let visible = previous_rect
        .map(|rect| clamp_point_to_rect(location, rect))
        .unwrap_or_else(|| clamp_point_to_outputs(location, output_rects));
    PointerConstraint {
        visible,
        raw: visible,
    }
}

fn rect_containing_point(
    point: Point<f64, Logical>,
    rects: &[Rectangle<i32, Logical>],
) -> Option<Rectangle<i32, Logical>> {
    rects
        .iter()
        .copied()
        .find(|rect| point_in_rect(point, *rect))
}

pub(super) fn rect_index_containing_point(
    point: Point<f64, Logical>,
    rects: &[Rectangle<i32, Logical>],
) -> Option<usize> {
    rects.iter().position(|rect| point_in_rect(point, *rect))
}

fn nearest_rect_to_point(
    point: Point<f64, Logical>,
    rects: &[Rectangle<i32, Logical>],
) -> Option<Rectangle<i32, Logical>> {
    rects.iter().copied().min_by(|a, b| {
        squared_distance(point, clamp_point_to_rect(point, *a))
            .partial_cmp(&squared_distance(point, clamp_point_to_rect(point, *b)))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn clamp_point_to_rect(
    location: Point<f64, Logical>,
    rect: Rectangle<i32, Logical>,
) -> Point<f64, Logical> {
    const EDGE_EPSILON: f64 = 0.000_001;
    let max_x = f64::from(rect.loc.x + rect.size.w.max(1)) - EDGE_EPSILON;
    let max_y = f64::from(rect.loc.y + rect.size.h.max(1)) - EDGE_EPSILON;
    Point::from((
        location.x.clamp(f64::from(rect.loc.x), max_x),
        location.y.clamp(f64::from(rect.loc.y), max_y),
    ))
}

fn squared_distance(a: Point<f64, Logical>, b: Point<f64, Logical>) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

fn point_in_rect(point: Point<f64, Logical>, rect: Rectangle<i32, Logical>) -> bool {
    point.x >= f64::from(rect.loc.x)
        && point.y >= f64::from(rect.loc.y)
        && point.x < f64::from(rect.loc.x + rect.size.w.max(1))
        && point.y < f64::from(rect.loc.y + rect.size.h.max(1))
}

pub(in crate::compositor::input) fn relative_pointer_location(
    raw_location: Point<f64, Logical>,
    delta: Point<f64, Logical>,
) -> Point<f64, Logical> {
    raw_location + delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_motion_starts_from_visible_location_after_pointer_hits_edge() {
        let output_rects = [Rectangle::from_size((100, 100).into())];
        let raw_location = Point::from((120.0, 50.0));
        let visible_location = clamp_point_to_outputs(raw_location, &output_rects);

        assert!((visible_location.x - 99.999_999).abs() < f64::EPSILON);
        assert_eq!(visible_location.y, 50.0);
        let relative = relative_pointer_location(visible_location, Point::from((20.0, 0.0)));
        assert!((relative.x - 119.999_999).abs() < f64::EPSILON);
        assert_eq!(relative.y, 50.0);
    }

    #[test]
    fn pointer_crosses_between_touching_outputs() {
        let output_rects = [
            Rectangle::new((0, 0).into(), (100, 100).into()),
            Rectangle::new((100, 0).into(), (100, 100).into()),
        ];

        let constrained = constrain_pointer_to_outputs(
            Point::from((99.999_999, 50.0)),
            Point::from((100.000_001, 50.0)),
            &output_rects,
        );

        assert_eq!(constrained.visible, Point::from((100.000_001, 50.0)));
        assert_eq!(constrained.raw, constrained.visible);
    }

    #[test]
    fn raw_pointer_does_not_accumulate_in_real_output_gaps() {
        let output_rects = [
            Rectangle::new((0, 0).into(), (100, 100).into()),
            Rectangle::new((120, 0).into(), (100, 100).into()),
        ];

        let constrained = constrain_pointer_to_outputs(
            Point::from((99.999_999, 50.0)),
            Point::from((110.0, 50.0)),
            &output_rects,
        );

        assert_eq!(constrained.visible, Point::from((99.999_999, 50.0)));
        assert_eq!(constrained.raw, constrained.visible);
    }

    #[test]
    fn pointer_accepts_locations_inside_any_output() {
        let output_rects = [
            Rectangle::new((0, 0).into(), (100, 100).into()),
            Rectangle::new((120, 0).into(), (100, 100).into()),
        ];

        let constrained = constrain_pointer_to_outputs(
            Point::from((99.999_999, 50.0)),
            Point::from((130.0, 50.0)),
            &output_rects,
        );

        assert_eq!(constrained.visible, Point::from((130.0, 50.0)));
        assert_eq!(constrained.raw, constrained.visible);
    }

    #[test]
    fn pointer_clamps_to_real_outputs_not_dead_space_in_bounding_box() {
        let output_rects = [
            Rectangle::new((0, 0).into(), (2560, 1440).into()),
            Rectangle::new((2560, 0).into(), (1366, 768).into()),
        ];

        let clamped = clamp_point_to_outputs(Point::from((3200.0, 1200.0)), &output_rects);
        assert_eq!(clamped.x, 3200.0);
        assert!((clamped.y - 767.999_999).abs() < f64::EPSILON);
    }

    #[test]
    fn primary_output_center_uses_first_active_output() {
        let output_rects = [
            Rectangle::new((100, 20).into(), (800, 600).into()),
            Rectangle::new((900, 20).into(), (640, 480).into()),
        ];

        assert_eq!(
            primary_output_center(&output_rects),
            Some(Point::from((500.0, 320.0)))
        );
    }
}
