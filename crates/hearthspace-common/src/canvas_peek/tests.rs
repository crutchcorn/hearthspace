use super::*;

pub(super) fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

pub(super) fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}

pub(super) fn window(id: u64, bounds: Rect) -> Window {
    Window {
        id,
        bounds,
        background: false,
        fixed: false,
    }
}

pub(super) fn icon(id: u64, x: f64, y: f64) -> Decoration {
    let bounds = rect(x, y, 88.0, 96.0);
    Decoration {
        id,
        bounds,
        body: icon_body(bounds),
        home: None,
        raised: false,
    }
}

pub(super) fn control(id: u64, bounds: Rect, raised: bool) -> Decoration {
    Decoration {
        id,
        bounds,
        body: bounds,
        home: None,
        raised,
    }
}

pub(super) fn world() -> Option<Rect> {
    Some(rect(0.0, 0.0, 1000.0, 800.0))
}

pub(super) fn position(rect: Rect) -> Point {
    point(rect.x, rect.y)
}

#[test]
fn adjacent_blockers_cover_the_union_but_a_gap_leaves_it_visible() {
    let body = rect(100.0, 100.0, 200.0, 200.0);
    let left = rect(100.0, 100.0, 100.0, 200.0);
    let right = rect(200.0, 100.0, 100.0, 200.0);
    assert!(fully_covered(body, &[left, right]));
    assert!(!fully_covered(
        body,
        &[
            Rect {
                width: 99.0,
                ..left
            },
            right
        ]
    ));
    assert!(!fully_covered(
        body,
        &[
            Rect {
                width: 99.999,
                ..left
            },
            right
        ]
    ));
}

#[test]
fn overlapping_blocker_areas_do_not_hide_an_uncovered_hole() {
    let body = rect(0.0, 0.0, 100.0, 100.0);
    let left = rect(0.0, 0.0, 60.0, 100.0);
    let top_right = rect(60.0, 0.0, 40.0, 40.0);
    assert!(!fully_covered(body, &[left, left, top_right]));
    assert!(fully_covered(
        body,
        &[left, top_right, rect(60.0, 40.0, 40.0, 60.0)]
    ));
}

#[test]
fn a_covered_window_uses_combined_coverage_without_moving_its_blockers() {
    let back = window(1, rect(100.0, 100.0, 200.0, 200.0));
    let left = window(2, rect(100.0, 100.0, 100.0, 200.0));
    let right = window(3, rect(200.0, 100.0, 100.0, 200.0));
    let mut scene = Scene {
        windows: vec![back, left, right],
        ..Scene::default()
    };
    settle(&mut scene, world());
    assert_eq!(scene.windows[0].bounds, rect(100.0, 76.0, 200.0, 200.0));
    assert_eq!(scene.windows[1..], [left, right]);
}

#[test]
fn partly_exposed_windows_stay_exactly_where_they_are() {
    let back = window(1, rect(100.0, 100.0, 200.0, 200.0));
    let front = window(2, rect(100.0, 100.001, 200.0, 200.0));
    let mut scene = Scene {
        windows: vec![back, front],
        ..Scene::default()
    };
    let original = scene.clone();
    settle(&mut scene, world());
    assert_eq!(scene, original);
}

#[test]
fn window_stack_settles_top_down_once_without_reordering() {
    let mut scene = Scene {
        windows: (1..=5)
            .map(|id| window(id, rect(100.0, 100.0, 200.0, 200.0)))
            .collect(),
        ..Scene::default()
    };
    settle(&mut scene, world());
    assert_eq!(
        scene
            .windows
            .iter()
            .map(|window| (window.id, position(window.bounds)))
            .collect::<Vec<_>>(),
        vec![
            (1, point(100.0, 124.0)),
            (2, point(124.0, 100.0)),
            (3, point(76.0, 100.0)),
            (4, point(100.0, 76.0)),
            (5, point(100.0, 100.0))
        ]
    );
    let settled = scene.clone();
    settle(&mut scene, world());
    assert_eq!(scene, settled);
}

#[test]
fn nested_window_chooses_the_nearest_edge() {
    let item = rect(260.0, 180.0, 100.0, 100.0);
    assert_eq!(
        reveal_position(
            item,
            item,
            &[rect(100.0, 100.0, 280.0, 300.0)],
            world(),
            WINDOW_PEEK
        ),
        Some(point(304.0, 180.0))
    );
}

#[test]
fn a_diagonal_opening_can_be_closer_than_any_unobstructed_edge() {
    let item = rect(100.0, 100.0, 100.0, 100.0);
    let blockers = [
        rect(0.0, 100.0, 1000.0, 100.0),
        rect(100.0, 0.0, 100.0, 800.0),
    ];
    assert_eq!(
        reveal_position(item, item, &blockers, world(), WINDOW_PEEK),
        Some(point(76.0, 76.0))
    );
}

#[test]
fn equal_edge_distances_prefer_up_then_left() {
    let item = rect(100.0, 100.0, 200.0, 200.0);
    assert_eq!(
        reveal_position(item, item, &[item], world(), WINDOW_PEEK),
        Some(point(100.0, 76.0))
    );
    let top_edge = Rect { y: 0.0, ..item };
    assert_eq!(
        reveal_position(top_edge, top_edge, &[top_edge], world(), WINDOW_PEEK),
        Some(point(76.0, 0.0))
    );
}

#[test]
fn finite_corner_and_infinite_plane_choose_different_nearest_openings() {
    let item = rect(0.0, 0.0, 200.0, 200.0);
    assert_eq!(
        reveal_position(item, item, &[item], world(), WINDOW_PEEK),
        Some(point(24.0, 0.0))
    );
    assert_eq!(
        reveal_position(item, item, &[item], None, WINDOW_PEEK),
        Some(point(0.0, -24.0))
    );
}

#[test]
fn finite_bounds_preserve_their_nonzero_negative_origin() {
    let item = rect(-500.0, -200.0, 200.0, 200.0);
    let bounds = Some(rect(-500.0, -200.0, 1000.0, 800.0));
    assert_eq!(
        reveal_position(item, item, &[item], bounds, WINDOW_PEEK),
        Some(point(-476.0, -200.0))
    );
}

#[test]
fn infinite_geometry_is_translation_invariant_for_negative_coordinates() {
    let item = rect(100.0, 100.0, 100.0, 100.0);
    let blockers = [
        rect(0.0, 100.0, 1000.0, 100.0),
        rect(100.0, 0.0, 100.0, 800.0),
    ];
    let shift = |rect: Rect| Rect {
        x: rect.x - 12345.0,
        y: rect.y - 67890.0,
        ..rect
    };
    let original = reveal_position(item, item, &blockers, None, WINDOW_PEEK).unwrap();
    let shifted = reveal_position(
        shift(item),
        shift(item),
        &blockers.map(shift),
        None,
        WINDOW_PEEK,
    )
    .unwrap();
    assert_eq!(shifted, point(original.x - 12345.0, original.y - 67890.0));
}

#[test]
fn infinite_search_reaches_beyond_any_viewport_or_fixed_radius() {
    let item = rect(0.0, 0.0, 200.0, 200.0);
    let blocker = rect(-1_000_000.0, -1_000_000.0, 2_000_200.0, 2_000_200.0);
    assert_eq!(
        reveal_position(item, item, &[blocker], None, WINDOW_PEEK),
        Some(point(0.0, -1_000_024.0))
    );
}

#[test]
fn impossible_finite_placement_does_not_move_or_resize_objects() {
    let back = window(1, rect(100.0, 100.0, 200.0, 200.0));
    let front = window(2, world().unwrap());
    let mut scene = Scene {
        windows: vec![back, front],
        ..Scene::default()
    };
    let original = scene.clone();
    settle(&mut scene, world());
    assert_eq!(scene, original);
    let oversized = rect(0.0, 0.0, 1001.0, 800.0);
    assert_eq!(
        reveal_position(oversized, oversized, &[oversized], world(), WINDOW_PEEK),
        None
    );
}

#[test]
fn a_small_object_reveals_its_whole_body() {
    let item = rect(100.0, 100.0, 10.0, 12.0);
    assert_eq!(
        reveal_position(item, item, &[item], world(), WINDOW_PEEK),
        Some(point(90.0, 100.0))
    );
}

#[test]
fn fractional_small_bodies_keep_ordered_placement_intervals() {
    let item = rect(0.05, 0.05, 1.0, 1.0);
    assert_eq!(
        reveal_position(item, item, &[item], None, WINDOW_PEEK),
        Some(point(0.05, -0.95))
    );
}

#[test]
fn fixed_windows_still_occlude_lower_windows() {
    let item = rect(100.0, 100.0, 200.0, 200.0);
    let fixed = Window {
        fixed: true,
        ..window(2, item)
    };
    let front = window(3, item);
    let mut scene = Scene {
        windows: vec![window(1, item), fixed, front],
        ..Scene::default()
    };
    settle(&mut scene, world());
    assert_eq!(position(scene.windows[0].bounds), point(100.0, 76.0));
    assert_eq!(scene.windows[1..], [fixed, front]);
}

#[test]
fn fixed_drop_obstacles_combine_with_windows_without_entering_the_stack() {
    let back = window(1, rect(100.0, 100.0, 200.0, 200.0));
    let right = Window {
        fixed: true,
        ..window(2, rect(200.0, 100.0, 100.0, 200.0))
    };
    let obstacle = Obstacle {
        bounds: rect(100.0, 100.0, 100.0, 200.0),
        background: true,
    };
    let mut scene = Scene {
        windows: vec![back, right],
        obstacles: vec![obstacle],
        ..Scene::default()
    };
    settle(&mut scene, world());
    assert_eq!(scene.windows.len(), 2);
    assert_eq!(position(scene.windows[0].bounds), point(100.0, 76.0));
    assert_eq!(scene.windows[1], right);
    assert_eq!(scene.obstacles, [obstacle]);
}
