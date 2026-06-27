use smithay::{
    reexports::winit::window::CursorIcon,
    utils::{Logical, Point, Rectangle, Size},
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;

use crate::{
    config::*,
    geometry::{CanvasPoint, rect_contains},
};

/// Which edges of a window are being dragged during an interactive resize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::compositor) struct ResizeEdges {
    pub(in crate::compositor) left: bool,
    pub(in crate::compositor) right: bool,
    pub(in crate::compositor) top: bool,
    pub(in crate::compositor) bottom: bool,
}

impl ResizeEdges {
    pub(in crate::compositor) fn is_empty(self) -> bool {
        !(self.left || self.right || self.top || self.bottom)
    }
}

impl From<xdg_toplevel::ResizeEdge> for ResizeEdges {
    fn from(edges: xdg_toplevel::ResizeEdge) -> Self {
        use xdg_toplevel::ResizeEdge;
        match edges {
            ResizeEdge::Top => Self {
                top: true,
                ..Self::default()
            },
            ResizeEdge::Bottom => Self {
                bottom: true,
                ..Self::default()
            },
            ResizeEdge::Left => Self {
                left: true,
                ..Self::default()
            },
            ResizeEdge::Right => Self {
                right: true,
                ..Self::default()
            },
            ResizeEdge::TopLeft => Self {
                top: true,
                left: true,
                ..Self::default()
            },
            ResizeEdge::TopRight => Self {
                top: true,
                right: true,
                ..Self::default()
            },
            ResizeEdge::BottomLeft => Self {
                bottom: true,
                left: true,
                ..Self::default()
            },
            ResizeEdge::BottomRight => Self {
                bottom: true,
                right: true,
                ..Self::default()
            },
            _ => Self::default(),
        }
    }
}

/// Canvas-space rectangle of a window's server-side title bar, given the
/// window's canvas position and the width of its content surface tree.
pub(super) fn title_bar_canvas_rect_for(
    position: CanvasPoint,
    content_width: i32,
) -> Rectangle<i32, Logical> {
    Rectangle::new(
        (position.x, position.y).into(),
        (content_width.max(MIN_WINDOW_WIDTH), TITLE_BAR_HEIGHT).into(),
    )
}

/// Canvas-space rectangle of the close button, positioned at the right edge of
/// the given title bar and vertically centered within it.
pub(super) fn close_button_canvas_rect_for(
    title_bar: Rectangle<i32, Logical>,
) -> Rectangle<i32, Logical> {
    Rectangle::new(
        (
            title_bar.loc.x + title_bar.size.w - CLOSE_BUTTON_MARGIN - CLOSE_BUTTON_SIZE,
            title_bar.loc.y + (title_bar.size.h - CLOSE_BUTTON_SIZE) / 2,
        )
            .into(),
        (CLOSE_BUTTON_SIZE, CLOSE_BUTTON_SIZE).into(),
    )
}

/// Canvas-space origin of a window's client content, offset below the title bar
/// only when the compositor is drawing server-side chrome for it.
pub(super) fn content_canvas_origin_for(
    position: CanvasPoint,
    has_chrome: bool,
) -> Point<i32, Logical> {
    let title_bar_height = if has_chrome { TITLE_BAR_HEIGHT } else { 0 };
    Point::<i32, Logical>::from((position.x, position.y + title_bar_height))
}

/// Full canvas-space bounds of a window given its position, client content size
/// and whether the compositor draws server-side chrome (a title bar above the
/// content). The width tracks the title bar, which is clamped to a minimum.
pub(super) fn window_canvas_rect_for(
    position: CanvasPoint,
    content_size: Size<i32, Logical>,
    has_chrome: bool,
) -> Rectangle<i32, Logical> {
    let title_bar_height = if has_chrome { TITLE_BAR_HEIGHT } else { 0 };
    let width = if has_chrome {
        content_size.w.max(MIN_WINDOW_WIDTH)
    } else {
        content_size.w
    };
    Rectangle::new(
        (position.x, position.y).into(),
        (width, content_size.h + title_bar_height).into(),
    )
}

/// Determine which resize edges, if any, the pointer is over. The resize handle
/// is a band centered on each window edge: it reaches `outset` pixels outside
/// the edge and `inset` pixels inside it, so the visible edge is grabbable while
/// the deep interior is not. Title bar and close-button hit-testing run first,
/// so they keep priority over the top resize band.
pub(super) fn resize_edges_at(
    window_rect: Rectangle<i32, Logical>,
    point: Point<f64, Logical>,
    outset: i32,
    inset: i32,
) -> Option<ResizeEdges> {
    let outer = Rectangle::new(
        (window_rect.loc.x - outset, window_rect.loc.y - outset).into(),
        (
            window_rect.size.w + outset * 2,
            window_rect.size.h + outset * 2,
        )
            .into(),
    );
    if !rect_contains(outer, point) {
        return None;
    }
    let inset = f64::from(inset);
    let edges = ResizeEdges {
        left: point.x < f64::from(window_rect.loc.x) + inset,
        right: point.x >= f64::from(window_rect.loc.x + window_rect.size.w) - inset,
        top: point.y < f64::from(window_rect.loc.y) + inset,
        bottom: point.y >= f64::from(window_rect.loc.y + window_rect.size.h) - inset,
    };
    (!edges.is_empty()).then_some(edges)
}

/// Desired client content size for a resize drag, derived from the size at the
/// start of the resize and the canvas-space pointer delta. Edges that are not
/// being dragged leave their dimension unchanged; the result is clamped to the
/// minimum window dimensions.
pub(super) fn resize_target_content_size(
    edges: ResizeEdges,
    initial: Size<i32, Logical>,
    delta: Point<i32, Logical>,
) -> Size<i32, Logical> {
    let mut width = initial.w;
    let mut height = initial.h;
    if edges.left {
        width = initial.w - delta.x;
    }
    if edges.right {
        width = initial.w + delta.x;
    }
    if edges.top {
        height = initial.h - delta.y;
    }
    if edges.bottom {
        height = initial.h + delta.y;
    }
    (width.max(MIN_WINDOW_WIDTH), height.max(MIN_WINDOW_HEIGHT)).into()
}

/// Window position that keeps the edge opposite a left/top resize anchored in
/// place as the client's content size changes from `initial` to `actual`.
pub(super) fn resize_anchored_position(
    edges: ResizeEdges,
    anchor: CanvasPoint,
    initial: Size<i32, Logical>,
    actual: Size<i32, Logical>,
) -> CanvasPoint {
    let mut position = anchor;
    if edges.left {
        position.x = anchor.x + initial.w - actual.w;
    }
    if edges.top {
        position.y = anchor.y + initial.h - actual.h;
    }
    position
}

/// The cursor that communicates which resize a window edge or corner performs.
/// Edges map to the bidirectional CSS-style resize cursors; corners map to the
/// matching diagonal cursor.
pub(in crate::compositor) fn resize_cursor_icon(edges: ResizeEdges) -> CursorIcon {
    match (edges.top || edges.bottom, edges.left || edges.right) {
        (true, true) => {
            if (edges.top && edges.left) || (edges.bottom && edges.right) {
                CursorIcon::NwseResize
            } else {
                CursorIcon::NeswResize
            }
        }
        (true, false) => CursorIcon::NsResize,
        (false, true) => CursorIcon::EwResize,
        (false, false) => CursorIcon::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: i32, y: i32) -> CanvasPoint {
        CanvasPoint { x, y }
    }

    #[test]
    fn title_bar_uses_content_width_when_wider_than_minimum() {
        let rect = title_bar_canvas_rect_for(point(40, 50), MIN_WINDOW_WIDTH + 120);
        assert_eq!(rect.loc.x, 40);
        assert_eq!(rect.loc.y, 50);
        assert_eq!(rect.size.w, MIN_WINDOW_WIDTH + 120);
        assert_eq!(rect.size.h, TITLE_BAR_HEIGHT);
    }

    #[test]
    fn title_bar_clamps_to_minimum_window_width() {
        let rect = title_bar_canvas_rect_for(point(0, 0), 10);
        assert_eq!(rect.size.w, MIN_WINDOW_WIDTH);
    }

    #[test]
    fn close_button_sits_inside_the_title_bar_right_edge() {
        let title_bar = title_bar_canvas_rect_for(point(100, 200), 400);
        let close = close_button_canvas_rect_for(title_bar);

        assert_eq!(close.size.w, CLOSE_BUTTON_SIZE);
        assert_eq!(close.size.h, CLOSE_BUTTON_SIZE);
        // Right edge respects the margin.
        assert_eq!(
            close.loc.x + close.size.w,
            title_bar.loc.x + title_bar.size.w - CLOSE_BUTTON_MARGIN
        );
        // Fully contained within the title bar vertically.
        assert!(close.loc.y >= title_bar.loc.y);
        assert!(close.loc.y + close.size.h <= title_bar.loc.y + title_bar.size.h);
    }

    #[test]
    fn content_origin_drops_below_title_bar_only_with_chrome() {
        assert_eq!(
            content_canvas_origin_for(point(10, 20), true),
            Point::<i32, Logical>::from((10, 20 + TITLE_BAR_HEIGHT))
        );
        assert_eq!(
            content_canvas_origin_for(point(10, 20), false),
            Point::<i32, Logical>::from((10, 20))
        );
    }

    fn size(w: i32, h: i32) -> Size<i32, Logical> {
        (w, h).into()
    }

    fn fpoint(x: f64, y: f64) -> Point<f64, Logical> {
        (x, y).into()
    }

    fn edges(left: bool, right: bool, top: bool, bottom: bool) -> ResizeEdges {
        ResizeEdges {
            left,
            right,
            top,
            bottom,
        }
    }

    #[test]
    fn resize_edges_from_protocol_corners_set_two_sides() {
        use xdg_toplevel::ResizeEdge;
        assert_eq!(
            ResizeEdges::from(ResizeEdge::TopLeft),
            edges(true, false, true, false)
        );
        assert_eq!(
            ResizeEdges::from(ResizeEdge::BottomRight),
            edges(false, true, false, true)
        );
        assert_eq!(
            ResizeEdges::from(ResizeEdge::Right),
            edges(false, true, false, false)
        );
        assert!(ResizeEdges::from(ResizeEdge::None).is_empty());
    }

    #[test]
    fn window_rect_adds_title_bar_height_only_with_chrome() {
        let with_chrome = window_canvas_rect_for(point(10, 20), size(400, 300), true);
        assert_eq!(with_chrome.loc, Point::<i32, Logical>::from((10, 20)));
        assert_eq!(with_chrome.size.w, 400);
        assert_eq!(with_chrome.size.h, 300 + TITLE_BAR_HEIGHT);

        let without_chrome = window_canvas_rect_for(point(10, 20), size(400, 300), false);
        assert_eq!(without_chrome.size.h, 300);
    }

    #[test]
    fn window_rect_width_clamps_to_minimum_with_chrome() {
        let rect = window_canvas_rect_for(point(0, 0), size(10, 200), true);
        assert_eq!(rect.size.w, MIN_WINDOW_WIDTH);
    }

    #[test]
    fn resize_edges_at_detects_corner_outside_window() {
        let rect = Rectangle::new((100, 100).into(), (200, 150).into());
        // Just outside the top-left corner.
        assert_eq!(
            resize_edges_at(rect, fpoint(96.0, 96.0), 8, 8),
            Some(edges(true, false, true, false))
        );
        // Along the right edge only.
        assert_eq!(
            resize_edges_at(rect, fpoint(303.0, 175.0), 8, 8),
            Some(edges(false, true, false, false))
        );
    }

    #[test]
    fn resize_edges_at_ignores_interior_and_far_points() {
        let rect = Rectangle::new((100, 100).into(), (200, 150).into());
        // Inside the window content.
        assert_eq!(resize_edges_at(rect, fpoint(150.0, 150.0), 8, 8), None);
        // Beyond the outset frame.
        assert_eq!(resize_edges_at(rect, fpoint(50.0, 50.0), 8, 8), None);
    }

    #[test]
    fn resize_edges_at_handle_is_centered_on_the_edge() {
        let rect = Rectangle::new((100, 100).into(), (200, 150).into());
        // The left edge is at x = 100; with an 8px inset the handle reaches to
        // x = 108 inside the window, so a point a few pixels inside resizes.
        assert_eq!(
            resize_edges_at(rect, fpoint(104.0, 175.0), 8, 8),
            Some(edges(true, false, false, false))
        );
        // With an 8px outset it also reaches to x = 92 outside the window.
        assert_eq!(
            resize_edges_at(rect, fpoint(95.0, 175.0), 8, 8),
            Some(edges(true, false, false, false))
        );
        // Just past the inset (x = 109) is interior content, not a resize target.
        assert_eq!(resize_edges_at(rect, fpoint(109.0, 175.0), 8, 8), None);
        // Just inside the bottom edge resizes too (250 is the bottom; 246 is within 8).
        assert_eq!(
            resize_edges_at(rect, fpoint(200.0, 246.0), 8, 8),
            Some(edges(false, false, false, true))
        );
    }

    #[test]
    fn resize_target_grows_with_bottom_right_drag() {
        let result = resize_target_content_size(
            edges(false, true, false, true),
            size(400, 300),
            (60, 40).into(),
        );
        assert_eq!(result, size(460, 340));
    }

    #[test]
    fn resize_target_shrinks_with_left_drag_and_clamps() {
        // Dragging the left edge right shrinks the width.
        let result = resize_target_content_size(
            edges(true, false, false, false),
            size(400, 300),
            (30, 0).into(),
        );
        assert_eq!(result, size(370, 300));
        // Clamped to the minimum width regardless of how far the drag goes.
        let clamped = resize_target_content_size(
            edges(true, false, false, false),
            size(400, 300),
            (10_000, 0).into(),
        );
        assert_eq!(clamped.w, MIN_WINDOW_WIDTH);
    }

    #[test]
    fn anchored_position_keeps_right_and_bottom_fixed_on_left_top_resize() {
        // Initial right edge = 100 + 400 = 500, bottom = 100 + 300 = 400.
        let position = resize_anchored_position(
            edges(true, false, true, false),
            point(100, 100),
            size(400, 300),
            size(450, 320),
        );
        // New width 450 keeps right at 500 -> x = 50. New height 320 keeps bottom at 400 -> y = 80.
        assert_eq!(position, point(50, 80));
    }

    #[test]
    fn anchored_position_unchanged_for_right_bottom_resize() {
        let position = resize_anchored_position(
            edges(false, true, false, true),
            point(100, 100),
            size(400, 300),
            size(450, 320),
        );
        assert_eq!(position, point(100, 100));
    }

    #[test]
    fn resize_cursor_matches_edges_and_corners() {
        assert_eq!(
            resize_cursor_icon(edges(false, false, true, false)),
            CursorIcon::NsResize
        );
        assert_eq!(
            resize_cursor_icon(edges(false, false, false, true)),
            CursorIcon::NsResize
        );
        assert_eq!(
            resize_cursor_icon(edges(true, false, false, false)),
            CursorIcon::EwResize
        );
        assert_eq!(
            resize_cursor_icon(edges(true, false, true, false)),
            CursorIcon::NwseResize,
            "top-left corner"
        );
        assert_eq!(
            resize_cursor_icon(edges(false, true, false, true)),
            CursorIcon::NwseResize,
            "bottom-right corner"
        );
        assert_eq!(
            resize_cursor_icon(edges(false, true, true, false)),
            CursorIcon::NeswResize,
            "top-right corner"
        );
        assert_eq!(
            resize_cursor_icon(edges(true, false, false, true)),
            CursorIcon::NeswResize,
            "bottom-left corner"
        );
        assert_eq!(
            resize_cursor_icon(ResizeEdges::default()),
            CursorIcon::Default
        );
    }
}
