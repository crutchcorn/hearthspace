use super::*;

fn view() -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        width: 1280.0,
        height: 800.0,
    }
}

#[test]
fn field_follows_anchor_and_places_results_on_the_roomier_side() {
    let top = layout(
        view(),
        Point { x: 640.0, y: 200.0 },
        &[(88.0, 96.0); 4],
        false,
        false,
    );
    assert_eq!(
        top.field.rect,
        Rect {
            x: 430.0,
            y: 173.0,
            width: 420.0,
            height: 54.0
        }
    );
    assert!(!top.field.results_above);
    assert_eq!(
        top.placements[0],
        Placement {
            x: 425.0,
            y: 255.0,
            scale: 1.0,
            matches: true
        }
    );
    let bottom = layout(
        view(),
        Point {
            x: 1240.0,
            y: 760.0,
        },
        &[(800.0, 600.0); 3],
        true,
        true,
    );
    assert_eq!(bottom.field.rect.x, 836.0);
    assert_eq!(bottom.field.rect.y, 650.0);
    assert!(bottom.field.results_above);
    assert_eq!(bottom.field.handwriting_height, 145.0);
    assert!(
        bottom
            .placements
            .iter()
            .all(|p| p.y + 600.0 * p.scale <= 481.0)
    );
}

#[test]
fn crowded_results_fit_available_space_and_respect_output_origin() {
    for count in [1, 12, 100, 4096] {
        let output = Rect {
            x: -1280.0,
            y: 200.0,
            ..view()
        };
        let result = layout(
            output,
            Point {
                x: -640.0,
                y: 400.0,
            },
            &vec![(640.0, 480.0); count],
            true,
            false,
        );
        for p in result.placements {
            assert!(p.scale > 0.0 && p.scale <= 1.0);
            assert!(p.x >= -1256.00001 && p.x + 640.0 * p.scale <= -23.99999);
            assert!(p.y >= 451.0 && p.y + 480.0 * p.scale <= 904.00001);
        }
    }
}

#[test]
fn compact_and_tiny_outputs_keep_the_field_valid() {
    for (width, height) in [(760.0, 600.0), (400.0, 420.0), (80.0, 60.0)] {
        let result = layout(
            Rect {
                width,
                height,
                ..Rect::default()
            },
            Point::default(),
            &[],
            false,
            true,
        );
        let rect = result.field.rect;
        assert!(rect.x >= 0.0 && rect.y >= 0.0);
        assert!(rect.x + rect.width <= width && rect.y + rect.height <= height);
    }
}

#[test]
fn matching_uses_all_words_and_unicode_lowercase() {
    assert!(matches("  ÉDITOR  window", "Window: Éditor"));
    assert!(matches("", "Anything"));
    assert!(!matches("photo folder", "Photos app"));
}

#[test]
fn nonmatches_park_beyond_nearest_edge() {
    let left = park(
        Rect {
            x: 28.0,
            y: 200.0,
            width: 88.0,
            height: 96.0,
        },
        view(),
    );
    assert_eq!(left.x, -136.0);
    assert_eq!(left.y, 200.0);
    assert!(!left.matches);
    let top = park(
        Rect {
            x: 600.0,
            y: 0.0,
            width: 88.0,
            height: 96.0,
        },
        view(),
    );
    assert_eq!(top.y, -144.0);
}

#[test]
fn session_scoped_surfaces_round_trip_and_reject_malformed_ids() {
    for output in [None, Some("card0/DP-1 🖥")] {
        assert_eq!(
            canvas_search_from_app_id(&canvas_search_app_id(3, output)),
            Some((3, output.map(str::to_owned)))
        );
    }
    assert_eq!(
        canvas_search_from_app_id(&canvas_search_app_id(0, None)),
        None
    );
    assert_eq!(
        canvas_search_result_from_app_id(&canvas_search_result_app_id(4, "control:sound")),
        Some((4, "control:sound".into()))
    );
    assert_eq!(
        canvas_search_result_from_app_id("dev.hearthspace.canvas-search-result.session-4.item-ff"),
        None
    );
    assert_eq!(
        canvas_search_result_from_app_id(&canvas_search_result_app_id(4, "")),
        None
    );
}

#[test]
fn motion_easing_matches_browser_curve_and_trims_only_invisible_travel() {
    assert_eq!(motion::ease(0.0), 0.0);
    assert_eq!(motion::ease(1.0), 1.0);
    assert!((motion::ease(0.5) - 0.94).abs() < 0.02);
    let start = Rect {
        x: -10000.0,
        y: 300.0,
        width: 200.0,
        height: 150.0,
    };
    let end = Rect { x: 500.0, ..start };
    let (trimmed, final_rect) = motion::visible_travel(start, end, view()).unwrap();
    assert!((trimmed.x + 248.0).abs() < 0.001);
    assert_eq!(final_rect, end);
    assert_eq!(
        motion::visible_travel(
            start,
            Rect {
                x: -5000.0,
                ..start
            },
            view()
        ),
        None
    );
}
