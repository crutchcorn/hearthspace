//! Exercise App's rendered placement, rather than injecting final callback bounds.
use hearthspace_common::frame_layout::{LayoutTarget, ObjectPlacement, Rect, TargetKind};
use smithay::utils::{Logical, Rectangle};

use super::test_support::app::AppFixture;
use crate::{
    compositor::{
        ManagedWindowKind,
        layout_editor::{PlacedTarget, Target},
    },
    geometry::CanvasPoint,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
    Rectangle::new((x, y).into(), (w, h).into())
}

#[test]
fn shell_callbacks_use_rendered_output_origins_despite_stored_canvas_position() {
    let mut f = AppFixture::new();
    let geometry = rect(8, 6, 300, 100);
    let osd = f.window(ManagedWindowKind::ControlOsd, Some("B"), geometry);
    let panel = f.window(ManagedWindowKind::Launcher, Some("B"), geometry);
    let bar = f.window(ManagedWindowKind::ShellBar, Some("B"), geometry);
    let output_a = f.app.output_logical_rect_for_name(Some("A")).unwrap();
    let output_b = f.app.output_logical_rect_for_name(Some("B")).unwrap();

    for index in [osd, panel, bar] {
        assert_eq!(f.app.windows[index].position, CanvasPoint { x: 0, y: 0 });
        assert!(
            f.app.window_screen_bounds(index).overlaps(output_a),
            "the old canvas-based selection would choose A"
        );
        let origin = if index == bar {
            f.app.shell_bar_logical_origin(index).unwrap()
        } else {
            f.app.launcher_logical_origin(index).unwrap()
        };
        let rendered = Rectangle::new(origin + geometry.loc, geometry.size);
        assert_eq!(f.app.frame_callback_bounds(index), Some(rendered));
        assert!(rendered.overlaps(output_b));
        assert!(!rendered.overlaps(output_a));
        f.request_frame(index);
    }
    assert_eq!(f.send_output("A", 10), 0);
    f.assert_done(&[]);
    assert_eq!(f.send_output("B", 11), 3);
    f.assert_done(&[(osd, 11), (panel, 11), (bar, 11)]);

    // Canvas motion must not move output-pinned shell callback ownership.
    f.app.viewport_scale = 0.5;
    f.app.viewport_offset = CanvasPoint { x: 400, y: 200 };
    for index in [osd, panel, bar] {
        f.request_frame(index);
    }
    assert_eq!(f.send_output("A", 12), 0);
    assert_eq!(f.send_output("B", 13), 3);
    f.assert_done(&[(osd, 13), (panel, 13), (bar, 13)]);
}

#[test]
fn overflow_group_callbacks_follow_visible_crop_and_fully_clipped_fallback() {
    let mut f = AppFixture::new();
    let window = f.window(ManagedWindowKind::Normal, None, rect(0, 0, 600, 100));
    f.app.windows[window].position = CanvasPoint { x: 700, y: 100 };
    let id = f.app.windows[window].id;
    // A wide canvas group member extends into B, but its scroll viewport is on A.
    f.app.layout_editor.placed = vec![
        PlacedTarget {
            target: Target {
                output: "A".into(),
                target: LayoutTarget::new(TargetKind::Group, "overflow"),
            },
            rect: Rect {
                x: 600.,
                y: 100.,
                width: 180.,
                height: 140.,
            },
            placement: ObjectPlacement::canvas(),
            group: None,
            vertical: false,
        },
        PlacedTarget {
            target: Target {
                output: "A".into(),
                target: LayoutTarget::new(TargetKind::Window, id.to_string()),
            },
            rect: Rect {
                x: 700.,
                y: 100.,
                width: 600.,
                height: 100.,
            },
            placement: ObjectPlacement::canvas(),
            group: Some("overflow".into()),
            vertical: false,
        },
    ];
    let a = f.app.output_logical_rect_for_name(Some("A")).unwrap();
    let b = f.app.output_logical_rect_for_name(Some("B")).unwrap();
    let projected = f.app.window_screen_bounds(window);
    assert_eq!(projected, rect(700, 100, 600, 100));
    assert!(projected.intersection(b).unwrap().size.w > projected.intersection(a).unwrap().size.w);
    let clip = f
        .app
        .layout_window_clip(window)
        .expect("real group overflow must crop");
    assert_eq!(clip, rect(605, 105, 170, 130));
    assert_eq!(
        f.app.frame_callback_bounds(window),
        projected.intersection(clip)
    );
    f.request_frame(window);
    assert_eq!(f.send_output("B", 20), 0);
    assert_eq!(f.send_output("A", 21), 1);
    f.assert_done(&[(window, 21)]);

    // Fully scrolled-out content keeps fallback service; its uncropped body
    // must not select B, and an empty intersection must not suppress callbacks.
    f.app.windows[window].position.x = 900;
    assert!(f.app.window_screen_bounds(window).overlaps(b));
    assert_eq!(f.app.frame_callback_bounds(window), None);
    f.request_frame(window);
    assert_eq!(f.send_output("B", 22), 0);
    assert_eq!(f.send_output("A", 23), 1);
    f.assert_done(&[(window, 23)]);

    // Once detached from the clipped group, ordinary overlap chooses B again.
    f.app.layout_editor.placed.clear();
    f.request_frame(window);
    assert_eq!(f.send_output("A", 24), 0);
    assert_eq!(f.send_output("B", 25), 1);
    f.assert_done(&[(window, 25)]);
}
