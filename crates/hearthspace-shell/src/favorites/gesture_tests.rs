use super::*;

fn line(points: &[[f64; 2]]) -> Gesture {
    Gesture::from_points(points).unwrap()
}

fn circle(start: f64, direction: f64, samples: usize) -> Vec<Point> {
    (0..=samples)
        .map(|i| {
            let angle = start + direction * std::f64::consts::TAU * i as f64 / samples as f64;
            [150.0 + 90.0 * angle.cos(), 150.0 + 90.0 * angle.sin()]
        })
        .collect()
}

fn transform(points: &[Point], scale: f64, rotation: f64) -> Vec<Point> {
    let (sin, cos) = rotation.to_radians().sin_cos();
    points
        .iter()
        .map(|p| {
            [
                400.0 + scale * (cos * p[0] - sin * p[1]),
                -250.0 + scale * (sin * p[0] + cos * p[1]),
            ]
        })
        .collect()
}

#[test]
fn position_scale_speed_and_modest_rotation_are_tolerated() {
    let shape = [[0.0, 0.0], [80.0, 160.0], [160.0, 0.0]];
    let template = line(&shape);
    // Unevenly spaced input, a pause's duplicates, and slightly moved corners.
    let holdout = [
        [0.0, 4.0],
        [5.0, 12.0],
        [5.0, 12.0],
        [34.0, 64.0],
        [76.0, 156.0],
        [85.0, 140.0],
        [140.0, 45.0],
        [161.0, 0.0],
    ];
    for scale in [0.5, 1.0, 3.2] {
        for rotation in [-15.0, 0.0, 15.0] {
            let candidate = line(&transform(&holdout, scale, rotation));
            assert!(
                template.distance_to(&candidate) < MATCH_DISTANCE,
                "scale {scale}, rotation {rotation}: {}",
                template.distance_to(&candidate)
            );
        }
    }
}

#[test]
fn closed_shapes_allow_shifted_starts_and_either_direction() {
    let template = line(&circle(0.0, 1.0, 80));
    for start in [0.3, 1.1, 2.4, 4.6] {
        for direction in [-1.0, 1.0] {
            let candidate = line(&circle(start, direction, 37));
            assert!(candidate.is_closed());
            assert!(template.distance_to(&candidate) < 0.03);
        }
    }
}

#[test]
fn triangle_tolerates_new_start_and_imperfect_closure() {
    let template = line(&[[0.0, 150.0], [80.0, 0.0], [160.0, 150.0], [0.0, 150.0]]);
    let holdout = line(&[
        [40.0, 150.0],
        [0.0, 145.0],
        [78.0, 7.0],
        [155.0, 146.0],
        [47.0, 153.0],
    ]);
    assert!(holdout.is_closed());
    assert!(template.distance_to(&holdout) < MATCH_DISTANCE);
}

#[test]
fn open_shape_can_be_drawn_in_reverse_but_not_rotated_arbitrarily() {
    let template = line(&[[0.0, 0.0], [0.0, 150.0], [100.0, 150.0]]);
    let reversed = line(&[[100.0, 150.0], [0.0, 150.0], [0.0, 0.0]]);
    assert!(template.distance_to(&reversed) < 1e-12);
    let rotated = line(&transform(
        &[[0.0, 0.0], [0.0, 150.0], [100.0, 150.0]],
        1.0,
        90.0,
    ));
    assert!(template.distance_to(&rotated) > MATCH_DISTANCE);
}

#[test]
fn unrelated_holdouts_are_rejected_without_nearest_template_fallback() {
    let template = line(&[[0.0, 0.0], [80.0, 160.0], [160.0, 0.0]]);
    for points in [
        vec![[0.0, 0.0], [0.0, 160.0], [160.0, 160.0]],
        vec![[0.0, 0.0], [160.0, 0.0], [0.0, 160.0], [160.0, 160.0]],
        vec![[0.0, 0.0], [160.0, 0.0]],
        circle(0.0, 1.0, 80),
    ] {
        assert!(template.distance_to(&line(&points)) > MATCH_DISTANCE);
    }
}

#[test]
fn invalid_tiny_and_unbounded_strokes_are_rejected() {
    for points in [
        vec![],
        vec![[0.0, 0.0]],
        vec![[0.0, 0.0]; 10],
        vec![[0.0, 0.0], [3.0, 2.0]],
        vec![[0.0, 0.0], [f64::NAN, 100.0]],
        vec![[0.0, 0.0], [f64::INFINITY, 100.0]],
        vec![[0.0, 0.0], [1_000_001.0, 100.0]],
        vec![[0.0, 0.0]; MAX_STROKE_POINTS + 1],
    ] {
        assert!(Gesture::from_points(&points).is_err());
    }
    let scribble: Vec<_> = (0..100)
        .map(|i| [if i % 2 == 0 { 0.0 } else { 100.0 }, 0.0])
        .collect();
    assert!(Gesture::from_points(&scribble).is_err());
}

#[test]
fn normalization_and_storage_roundtrip_are_stable() {
    let gesture = line(&circle(0.4, -1.0, 37));
    assert_eq!(gesture.points().len(), SAMPLE_COUNT);
    assert!((extent(gesture.points()) - 1.0).abs() < 1e-12);
    assert!(centroid(gesture.points()).iter().all(|v| v.abs() < 1e-12));
    let encoded = serde_json::to_vec(&gesture).unwrap();
    let decoded: Gesture = serde_json::from_slice(&encoded).unwrap();
    assert!(gesture.distance_to(&decoded) < 1e-12);
    let mut malformed: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    malformed["points"][0][0] = serde_json::json!(50);
    assert!(serde_json::from_value::<Gesture>(malformed).is_err());
}
