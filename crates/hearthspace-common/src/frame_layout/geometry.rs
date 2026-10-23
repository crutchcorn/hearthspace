//! Logical-pixel geometry shared by compositor input, native shell, and regression tests.
use super::{FrameEdge, ObjectPlacement};
pub use crate::canvas_search::{Point, Rect};

pub const GUIDE_INSET: f64 = 8.0;
pub const GUIDE_GAP: f64 = 8.0;
pub const PINNED_EXPOSED_SIZE: f64 = 42.0;
pub const PINNED_HOVER_SIZE: f64 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrameCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl FrameCorner {
    pub fn is_left(self) -> bool {
        matches!(self, Self::TopLeft | Self::BottomLeft)
    }
    pub fn is_top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum GuideZone {
    Edge(FrameEdge),
    Corner(FrameCorner),
}

pub fn guide_size(viewport: Rect) -> f64 {
    72.0_f64
        .min((viewport.width - 32.0) / 2.0)
        .min((viewport.height - 32.0) / 2.0)
        .max(0.0)
}

pub fn guide_rects(viewport: Rect) -> [(GuideZone, Rect); 8] {
    let size = guide_size(viewport);
    let x = GUIDE_INSET;
    let y = GUIDE_INSET;
    let right = (viewport.width - GUIDE_INSET - size).max(x);
    let bottom = (viewport.height - GUIDE_INSET - size).max(y);
    let middle_x = x + size + GUIDE_GAP;
    let middle_y = y + size + GUIDE_GAP;
    let middle_width = (right - GUIDE_GAP - middle_x).max(0.0);
    let middle_height = (bottom - GUIDE_GAP - middle_y).max(0.0);
    [
        (
            GuideZone::Edge(FrameEdge::Top),
            Rect {
                x: middle_x,
                y,
                width: middle_width,
                height: size,
            },
        ),
        (
            GuideZone::Edge(FrameEdge::Right),
            Rect {
                x: right,
                y: middle_y,
                width: size,
                height: middle_height,
            },
        ),
        (
            GuideZone::Edge(FrameEdge::Bottom),
            Rect {
                x: middle_x,
                y: bottom,
                width: middle_width,
                height: size,
            },
        ),
        (
            GuideZone::Edge(FrameEdge::Left),
            Rect {
                x,
                y: middle_y,
                width: size,
                height: middle_height,
            },
        ),
        (
            GuideZone::Corner(FrameCorner::TopLeft),
            Rect {
                x,
                y,
                width: size,
                height: size,
            },
        ),
        (
            GuideZone::Corner(FrameCorner::TopRight),
            Rect {
                x: right,
                y,
                width: size,
                height: size,
            },
        ),
        (
            GuideZone::Corner(FrameCorner::BottomLeft),
            Rect {
                x,
                y: bottom,
                width: size,
                height: size,
            },
        ),
        (
            GuideZone::Corner(FrameCorner::BottomRight),
            Rect {
                x: right,
                y: bottom,
                width: size,
                height: size,
            },
        ),
    ]
}

pub fn frame_corner_at_pointer(pointer: Point, viewport: Rect) -> Option<FrameCorner> {
    guide_rects(viewport).into_iter().find_map(|(zone, rect)| {
        if let GuideZone::Corner(corner) = zone {
            (pointer.x >= rect.x
                && pointer.x <= rect.x + rect.width
                && pointer.y >= rect.y
                && pointer.y <= rect.y + rect.height)
                .then_some(corner)
        } else {
            None
        }
    })
}

/// Position is normalized along an edge. Widgets use padding; pinned windows meet the edge.
pub fn frame_bounds(
    placement: ObjectPlacement,
    width: f64,
    height: f64,
    viewport: Rect,
    padded: bool,
) -> Rect {
    let ObjectPlacement::Frame { edge, position } = placement else {
        return Rect {
            width,
            height,
            ..Rect::default()
        };
    };
    let margin = if viewport.width <= 760.0 {
        12.0_f64
    } else {
        24.0_f64
    };
    let inset_x = margin.min(viewport.width / 2.0);
    let inset_y = margin.min(viewport.height / 2.0);
    let width = width.min((viewport.width - inset_x * 2.0).max(0.0));
    let height = height.min((viewport.height - inset_y * 2.0).max(0.0));
    let position = position.clamp(0.0, 1.0);
    let x = if !edge.is_vertical() {
        inset_x + (viewport.width - inset_x * 2.0 - width) * position
    } else if edge == FrameEdge::Right {
        viewport.width - width - if padded { inset_x } else { 0.0 }
    } else if padded {
        inset_x
    } else {
        0.0
    };
    let y = if edge.is_vertical() {
        inset_y + (viewport.height - inset_y * 2.0 - height) * position
    } else if edge == FrameEdge::Bottom {
        viewport.height - height - if padded { inset_y } else { 0.0 }
    } else if padded {
        inset_y
    } else {
        0.0
    };
    Rect {
        x,
        y,
        width,
        height,
    }
}

pub fn pinned_window_offset(
    bounds: Rect,
    edge: FrameEdge,
    viewport: Rect,
    expanded: bool,
    exposed_size: f64,
) -> Point {
    if expanded {
        return Point::default();
    }
    let width = exposed_size.min(bounds.width).max(0.0);
    let height = exposed_size.min(bounds.height).max(0.0);
    Point {
        x: match edge {
            FrameEdge::Left => -(bounds.x + bounds.width - width),
            FrameEdge::Right => viewport.width - bounds.x - width,
            _ => 0.0,
        },
        y: match edge {
            FrameEdge::Top => -(bounds.y + bounds.height - height),
            FrameEdge::Bottom => viewport.height - bounds.y - height,
            _ => 0.0,
        },
    }
}

pub fn frame_drag_bounds(bounds: Rect, viewport: Rect) -> Rect {
    let x = bounds.x.max(0.0);
    let y = bounds.y.max(0.0);
    Rect {
        x,
        y,
        width: (viewport.width.min(bounds.x + bounds.width) - x).max(0.0),
        height: (viewport.height.min(bounds.y + bounds.height) - y).max(0.0),
    }
}

pub fn frame_drop_target(
    bounds: Rect,
    viewport: Rect,
    maximum_distance: f64,
    pointer: Option<Point>,
) -> Option<ObjectPlacement> {
    let distances = [
        (FrameEdge::Top, bounds.y),
        (FrameEdge::Right, viewport.width - bounds.x - bounds.width),
        (
            FrameEdge::Bottom,
            viewport.height - bounds.y - bounds.height,
        ),
        (FrameEdge::Left, bounds.x),
    ];
    // Strict comparison deliberately retains top/right/bottom/left tie order from the site.
    let (edge, distance) = distances.into_iter().reduce(|nearest, candidate| {
        if candidate.1 < nearest.1 {
            candidate
        } else {
            nearest
        }
    })?;
    if let Some(corner) = pointer.and_then(|point| frame_corner_at_pointer(point, viewport)) {
        return Some(corner_placement(corner, edge.is_vertical()));
    }
    if distance > maximum_distance {
        return None;
    }
    let margin = if viewport.width <= 760.0 { 12.0 } else { 24.0 };
    let (extent, original_size, start) = if edge.is_vertical() {
        (viewport.height, bounds.height, bounds.y)
    } else {
        (viewport.width, bounds.width, bounds.x)
    };
    let size = original_size.min((extent - margin * 2.0).max(0.0));
    let span = extent - margin * 2.0 - size;
    let position = if span <= 0.0 {
        0.5
    } else {
        ((start + original_size / 2.0 - margin - size / 2.0) / span).clamp(0.0, 1.0)
    };
    Some(ObjectPlacement::Frame { edge, position })
}

/// Corner drops keep the incoming preview's axis; ordinary edges may turn it.
/// `current_edge` must follow the active drag, rather than its saved starting placement.
pub fn widget_frame_drop_target(
    bounds: Rect,
    viewport: Rect,
    maximum_distance: f64,
    current_edge: Option<FrameEdge>,
    pointer: Option<Point>,
) -> Option<ObjectPlacement> {
    let size = bounds.width.min(bounds.height);
    let anchor = pointer.unwrap_or(Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    });
    let target = frame_drop_target(
        Rect {
            x: anchor.x - size / 2.0,
            y: anchor.y - size / 2.0,
            width: size,
            height: size,
        },
        viewport,
        maximum_distance,
        Some(anchor),
    )?;
    Some(
        frame_corner_at_pointer(anchor, viewport)
            .map(|corner| {
                corner_placement(corner, current_edge.is_some_and(FrameEdge::is_vertical))
            })
            .unwrap_or(target),
    )
}

fn corner_placement(corner: FrameCorner, vertical: bool) -> ObjectPlacement {
    let edge = if vertical {
        if corner.is_left() {
            FrameEdge::Left
        } else {
            FrameEdge::Right
        }
    } else if corner.is_top() {
        FrameEdge::Top
    } else {
        FrameEdge::Bottom
    };
    let position = if if vertical {
        corner.is_top()
    } else {
        corner.is_left()
    } {
        0.0
    } else {
        1.0
    };
    ObjectPlacement::Frame { edge, position }
}

/// Snap the frame anchor to the same canvas grid without moving its exact corner endpoints.
pub fn snap_frame_placement(
    placement: ObjectPlacement,
    size: Rect,
    viewport: Rect,
    canvas_origin: Point,
    grid: f64,
    padded: bool,
) -> ObjectPlacement {
    let ObjectPlacement::Frame { edge, position } = placement else {
        return placement;
    };
    if grid <= 0.0 || !grid.is_finite() || position == 0.0 || position == 1.0 {
        return placement;
    }
    let axis = |rect: Rect| if edge.is_vertical() { rect.y } else { rect.x };
    let start = axis(frame_bounds(
        ObjectPlacement::frame(edge, 0.0),
        size.width,
        size.height,
        viewport,
        padded,
    ));
    let end = axis(frame_bounds(
        ObjectPlacement::frame(edge, 1.0),
        size.width,
        size.height,
        viewport,
        padded,
    ));
    if end <= start {
        return placement;
    }
    let origin = if edge.is_vertical() {
        canvas_origin.y
    } else {
        canvas_origin.x
    };
    let current = axis(frame_bounds(
        placement,
        size.width,
        size.height,
        viewport,
        padded,
    ));
    let snapped = (((current + origin) / grid).round() * grid - origin).clamp(start, end);
    ObjectPlacement::frame(edge, (snapped - start) / (end - start))
}

#[cfg(test)]
#[path = "corner_tests.rs"]
mod corner_tests;
