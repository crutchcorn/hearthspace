use super::*;

fn polygon(points: &[(f64, f64)]) -> Lasso {
    let mut lasso = Lasso::new(points[0].into());
    for &point in &points[1..] {
        lasso.push(point.into());
    }
    lasso
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
    Rectangle::new((x, y).into(), (w, h).into())
}

#[test]
fn concave_polygon_excludes_bounding_box_notch_in_both_directions() {
    let points = [
        (0., 0.),
        (100., 0.),
        (100., 30.),
        (30., 30.),
        (30., 100.),
        (0., 100.),
    ];
    for points in [points.to_vec(), points.into_iter().rev().collect()] {
        let lasso = polygon(&points);
        assert!(lasso.selects(rect(5, 40, 10, 10)));
        assert!(lasso.selects(rect(40, 5, 10, 10)));
        assert!(!lasso.selects(rect(50, 50, 20, 20)));
        assert!(!lasso.selects(rect(200, 10, 10, 10)));
    }
}

#[test]
fn intersection_covers_containment_edges_and_segment_crossings() {
    let lasso = polygon(&[(0., 0.), (100., 0.), (100., 100.), (0., 100.)]);
    assert!(lasso.selects(rect(-10, -10, 120, 120)));
    assert!(lasso.selects(rect(10, 10, 10, 10)));
    assert!(lasso.selects(rect(100, 20, 10, 10)));
    assert!(lasso.selects(rect(-20, 40, 140, 10)));
    assert!(!lasso.selects(rect(101, 20, 10, 10)));
    assert!(!lasso.selects(rect(20, 20, 0, 10)));
}

#[test]
fn degenerate_and_retraced_paths_do_not_select() {
    for points in [
        vec![(20., 20.)],
        vec![(0., 0.), (100., 100.)],
        vec![(0., 0.), (50., 50.), (100., 100.)],
        vec![(0., 0.), (100., 0.), (50., 100.), (100., 0.), (0., 0.)],
        vec![
            (0., 0.),
            (100., 0.),
            (100., 100.),
            (0., 100.),
            (0., 0.),
            (100., 0.),
            (100., 100.),
            (0., 100.),
        ],
    ] {
        assert!(!polygon(&points).selects(rect(-10, -10, 200, 200)));
    }
}

#[test]
fn self_crossing_loops_follow_even_odd_fill() {
    let lasso = polygon(&[(0., 0.), (100., 100.), (0., 100.), (100., 0.)]);
    assert!(lasso.selects(rect(40, 10, 10, 10)));
    assert!(lasso.selects(rect(40, 80, 10, 10)));
    assert!(!lasso.selects(rect(0, 40, 10, 10)));
}

#[test]
fn long_gestures_are_bounded_and_keep_origin_and_live_endpoint() {
    let mut lasso = Lasso::new((0., 0.).into());
    for index in 1..100_000 {
        lasso.push((f64::from(index), (f64::from(index) / 40.).sin() * 100.).into());
        assert!(lasso.points().len() <= MAX_POINTS);
    }
    assert_eq!(lasso.points()[0], (0., 0.).into());
    assert_eq!(lasso.points().last().unwrap().x, 99_999.);
}

#[test]
fn coordinates_are_finite_bounded_and_safe_near_rectangle_limits() {
    let mut lasso = Lasso::new((f64::NAN, 0.).into());
    lasso.push((f64::INFINITY, f64::NEG_INFINITY).into());
    assert!(lasso.points().is_empty());
    for point in [(-1e30, -1e30), (1e30, -1e30), (1e30, 1e30), (-1e30, 1e30)] {
        lasso.push(point.into());
    }
    assert!(
        lasso
            .points()
            .iter()
            .all(|point| point.x.abs() <= COORDINATE_LIMIT && point.y.abs() <= COORDINATE_LIMIT)
    );
    assert!(lasso.selects(rect(-100, -100, 200, 200)));
    assert!(!lasso.selects(rect(i32::MAX - 50, i32::MIN, 100, 100)));
}
