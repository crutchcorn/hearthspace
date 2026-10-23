//! Pointer probes preserve the incoming axis only inside the visible corner guides.
use super::*;

const VIEW: Rect = Rect {
    x: 0.,
    y: 0.,
    width: 1280.,
    height: 800.,
};

#[test]
fn every_corner_keeps_either_incoming_axis_for_both_toolbar_shapes() {
    use FrameEdge::{Bottom, Left, Right, Top};
    for (name, pointer, horizontal, vertical, horizontal_position, vertical_position) in [
        ("top-left", Point { x: 45., y: 45. }, Top, Left, 0., 0.),
        ("top-right", Point { x: 1235., y: 45. }, Top, Right, 1., 0.),
        (
            "bottom-left",
            Point { x: 45., y: 755. },
            Bottom,
            Left,
            0.,
            1.,
        ),
        (
            "bottom-right",
            Point { x: 1235., y: 755. },
            Bottom,
            Right,
            1.,
            1.,
        ),
    ] {
        for incoming in [Top, Right, Bottom, Left] {
            let expected = if incoming.is_vertical() {
                ObjectPlacement::frame(vertical, vertical_position)
            } else {
                ObjectPlacement::frame(horizontal, horizontal_position)
            };
            // Reflow can lag the pointer: neither the old dimensions nor an
            // off-center grab should override the preview's remembered axis.
            for (width, height) in [(299., 54.), (54., 299.)] {
                let bounds = Rect {
                    x: pointer.x - 16.,
                    y: pointer.y - 20.,
                    width,
                    height,
                };
                for distance in [72., f64::INFINITY] {
                    assert_eq!(
                        widget_frame_drop_target(
                            bounds,
                            VIEW,
                            distance,
                            Some(incoming),
                            Some(pointer)
                        ),
                        Some(expected),
                        "{name}, incoming {incoming:?}, {width}x{height}"
                    );
                }
            }
        }
    }
}

#[test]
fn leaving_a_corner_changes_axis_only_after_the_pointer_crosses_the_guide() {
    let bounds = Rect {
        x: 20.,
        y: 20.,
        width: 299.,
        height: 54.,
    };
    for (pointer, edge) in [
        (Point { x: 80., y: 45. }, FrameEdge::Left),
        (Point { x: 81., y: 45. }, FrameEdge::Top),
    ] {
        let target =
            widget_frame_drop_target(bounds, VIEW, 72., Some(FrameEdge::Left), Some(pointer));
        assert_eq!(target.and_then(ObjectPlacement::edge), Some(edge));
    }
    for (pointer, edge) in [
        (Point { x: 45., y: 80. }, FrameEdge::Top),
        (Point { x: 45., y: 81. }, FrameEdge::Left),
    ] {
        let target =
            widget_frame_drop_target(bounds, VIEW, 72., Some(FrameEdge::Top), Some(pointer));
        assert_eq!(target.and_then(ObjectPlacement::edge), Some(edge));
    }
}

#[test]
fn interior_probe_does_not_attach_a_long_toolbar_merely_touching_the_frame() {
    let pointer = Point { x: 640., y: 400. };
    for (width, height) in [(1200., 54.), (54., 760.)] {
        assert_eq!(
            widget_frame_drop_target(
                Rect {
                    x: pointer.x - width / 2.,
                    y: pointer.y - height / 2.,
                    width,
                    height
                },
                VIEW,
                72.,
                Some(FrameEdge::Left),
                Some(pointer),
            ),
            None
        );
    }
}
