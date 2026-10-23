//! Snap moving visible edges while leaving the opposite edges fixed.
use smithay::utils::{Logical, Point, Rectangle, Size};

use super::ResizeEdges;
use crate::geometry::grid::snap_coordinate;

pub(super) fn target_size(
    bounds: Rectangle<i32, Logical>,
    initial: Size<i32, Logical>,
    edges: ResizeEdges,
    delta: Point<f64, Logical>,
    min: Size<i32, Logical>,
    max: Size<i32, Logical>,
    snap: bool,
) -> Size<i32, Logical> {
    let axis = |origin: i32,
                span: i32,
                content: i32,
                leading: bool,
                trailing: bool,
                delta: f64,
                min: i32,
                max: i32| {
        if !leading && !trailing {
            return content;
        }
        let edge = f64::from(origin) + if leading { 0.0 } else { f64::from(span) };
        let content = f64::from(content);
        let min = f64::from(min);
        let max = f64::from(max);
        let target = if snap {
            let (low, high) = if leading {
                (edge + content - max, edge + content - min)
            } else {
                (edge - content + min, edge - content + max)
            };
            snap_coordinate(edge + delta, low, high) - edge
        } else {
            delta.round()
        };
        (content + if leading { -target } else { target }).clamp(min, max) as i32
    };
    (
        axis(
            bounds.loc.x,
            bounds.size.w,
            initial.w,
            edges.left,
            edges.right,
            delta.x,
            min.w,
            max.w,
        ),
        axis(
            bounds.loc.y,
            bounds.size.h,
            initial.h,
            edges.top,
            edges.bottom,
            delta.y,
            min.h,
            max.h,
        ),
    )
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge;

    #[test]
    fn all_edges_snap_visible_bounds_with_server_titlebar_and_fixed_opposite_edges() {
        let bounds = Rectangle::new((103, 209).into(), (400, 300).into());
        let content = (400, 260).into();
        for (edge, width, height) in [
            (ResizeEdge::Left, 377, 260),
            (ResizeEdge::Right, 415, 260),
            (ResizeEdge::Top, 400, 287),
            (ResizeEdge::Bottom, 400, 241),
            (ResizeEdge::TopLeft, 377, 287),
            (ResizeEdge::TopRight, 415, 287),
            (ResizeEdge::BottomLeft, 377, 241),
            (ResizeEdge::BottomRight, 415, 241),
        ] {
            assert_eq!(
                target_size(
                    bounds,
                    content,
                    edge.into(),
                    (23.0, -17.0).into(),
                    (260, 120).into(),
                    (1000, 1000).into(),
                    true
                ),
                (width, height).into(),
                "{edge:?}"
            );
        }
    }

    #[test]
    fn constrained_resize_uses_reachable_dots_or_the_client_limit() {
        let bounds = Rectangle::new((-103, -209).into(), (400, 300).into());
        let edges = ResizeEdge::BottomRight.into();
        // Rightmost reachable dot with a 410px maximum is x=294, not x=322.
        assert_eq!(
            target_size(
                bounds,
                bounds.size,
                edges,
                (50.0, 50.0).into(),
                (260, 120).into(),
                (410, 320).into(),
                true
            ),
            (397, 307).into()
        );
        // The width interval contains no dot; fixed app dimensions win.
        assert_eq!(
            target_size(
                bounds,
                bounds.size,
                edges,
                (50.0, 50.0).into(),
                (400, 300).into(),
                (400, 300).into(),
                true
            ),
            bounds.size
        );
        assert_eq!(
            target_size(
                bounds,
                bounds.size,
                edges,
                (23.0, -17.0).into(),
                (260, 120).into(),
                (1000, 1000).into(),
                false
            ),
            (423, 283).into()
        );
    }
}
