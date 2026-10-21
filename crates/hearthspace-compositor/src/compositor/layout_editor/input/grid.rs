//! Convert the canonical dot grid to an output-local frame anchor grid.
use super::{Point, Rect};
use crate::geometry::{CanvasPoint, grid::CANVAS_GRID_OFFSET};

pub(super) fn origin(offset: CanvasPoint, scale: f64, view: Rect) -> Point {
    Point {
        x: view.x + f64::from(offset.x - CANVAS_GRID_OFFSET) * scale,
        y: view.y + f64::from(offset.y - CANVAS_GRID_OFFSET) * scale,
    }
}

/// A frame candidate constrains snapping to its parallel axis; canvas drops use both axes.
pub(super) fn align(
    x: f64,
    y: f64,
    snap: bool,
    edge: Option<hearthspace_common::frame_layout::FrameEdge>,
) -> (f64, f64) {
    let coordinate = |value: f64, enabled: bool| {
        if snap && enabled {
            crate::geometry::grid::snap_coordinate(value, -1_000_000., 1_000_000.)
        } else {
            value.clamp(-1_000_000., 1_000_000.)
        }
    };
    (
        coordinate(x, !edge.is_some_and(|edge| edge.is_vertical())),
        coordinate(y, edge.is_none_or(|edge| edge.is_vertical())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::grid::CANVAS_GRID_SIZE;
    use hearthspace_common::frame_layout::{
        FrameEdge, ObjectPlacement, frame_bounds, snap_frame_placement,
    };

    #[test]
    fn frame_candidate_snaps_only_parallel_motion() {
        assert_eq!(align(-19., 31., true, Some(FrameEdge::Left)), (-19., 42.));
        assert_eq!(align(-19., 31., true, Some(FrameEdge::Top)), (-14., 31.));
        assert_eq!(align(-19., 31., true, None), (-14., 42.));
        assert_eq!(align(-19., 31., false, None), (-19., 31.));
    }

    #[test]
    fn frame_anchor_matches_canvas_dots_after_zoom_pan_and_output_translation() {
        let view = Rect {
            x: 1280.,
            y: -200.,
            width: 1100.,
            height: 800.,
        };
        let offset = CanvasPoint { x: -157, y: -213 };
        let size = Rect {
            x: 0.,
            y: 0.,
            width: 100.,
            height: 54.,
        };
        for scale in [0.5, 1., 1.75] {
            for edge in [FrameEdge::Top, FrameEdge::Left] {
                let placement = snap_frame_placement(
                    ObjectPlacement::frame(edge, 0.48),
                    size,
                    view,
                    origin(offset, scale, view),
                    f64::from(CANVAS_GRID_SIZE) * scale,
                    true,
                );
                let rect = frame_bounds(placement, size.width, size.height, view, true);
                let canonical = if edge.is_vertical() {
                    (rect.y + view.y) / scale + f64::from(offset.y)
                } else {
                    (rect.x + view.x) / scale + f64::from(offset.x)
                };
                let dot = (canonical - f64::from(CANVAS_GRID_OFFSET)) / f64::from(CANVAS_GRID_SIZE);
                assert!(
                    (dot - dot.round()).abs() < 1e-8,
                    "{scale} {edge:?}: {canonical}"
                );
            }
        }
    }
}
