use crate::frame_layout::{Point, Rect};

const EPSILON: f64 = 0.000_001;

#[derive(Clone, Copy)]
struct Interval {
    start: f64,
    end: f64,
}

#[derive(Clone, Copy)]
struct PatchBlocker {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

pub(super) fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let width = (a.x + a.width).min(b.x + b.width) - x;
    let height = (a.y + a.height).min(b.y + b.height) - y;
    (width > EPSILON && height > EPSILON).then_some(Rect {
        x,
        y,
        width,
        height,
    })
}

/// Whether the union of opaque rectangles covers the entire body.
///
/// Edge contact alone is not coverage. The reference's one-millionth-pixel
/// tolerance applies to intersections and gaps.
pub fn fully_covered(body: Rect, higher: &[Rect]) -> bool {
    if !valid_rect(body) {
        return false;
    }
    let overlaps: Vec<_> = higher
        .iter()
        .copied()
        .filter(|rect| valid_rect(*rect))
        .filter_map(|other| intersect(body, other))
        .collect();
    if overlaps.is_empty() {
        return false;
    }
    let mut edges = vec![body.x, body.x + body.width];
    edges.extend(
        overlaps
            .iter()
            .flat_map(|item| [item.x, item.x + item.width]),
    );
    edges.sort_by(f64::total_cmp);
    edges.dedup();
    // Coverage can change only at a vertical edge of an intersecting rectangle.
    for pair in edges.windows(2) {
        if pair[1] - pair[0] <= EPSILON {
            continue;
        }
        let x = pair[0] + (pair[1] - pair[0]) / 2.0;
        let mut intervals: Vec<_> = overlaps
            .iter()
            .filter(|item| item.x < x && item.x + item.width > x)
            .map(|item| Interval {
                start: item.y,
                end: item.y + item.height,
            })
            .collect();
        intervals.sort_by(|a, b| a.start.total_cmp(&b.start));
        let mut covered_to = body.y;
        for interval in intervals {
            if interval.start > covered_to + EPSILON {
                return false;
            }
            covered_to = covered_to.max(interval.end);
        }
        if covered_to < body.y + body.height - EPSILON {
            return false;
        }
    }
    true
}

/// Find the nearest object position exposing an unoccluded rectangular patch.
///
/// `body` is artwork in the same coordinates as `item`, translated with it.
/// The patch is `min(peek, body.width)` by `min(peek, body.height)`. Ties prefer
/// smaller y, then smaller x. Bounds, when supplied, contain the whole item;
/// their origin need not be zero. Returns `None` for impossible placements or
/// invalid object geometry. Callers should first check `fully_covered` so an
/// already visible sliver never causes movement just to expose a larger patch.
pub fn reveal_position(
    item: Rect,
    body: Rect,
    higher: &[Rect],
    bounds: Option<Rect>,
    peek: f64,
) -> Option<Point> {
    if !valid_rect(item) || !valid_rect(body) || !peek.is_finite() || peek <= 0.0 {
        return None;
    }
    let width = peek.min(body.width);
    let height = peek.min(body.height);
    let span_x = body.width - width;
    let span_y = body.height - height;
    let offset_x = body.x - item.x;
    let offset_y = body.y - item.y;
    // A patch intersects a blocker only while its top-left lies strictly inside
    // this expanded rectangle. Its boundary represents an exposed patch.
    let blocked: Vec<_> = higher
        .iter()
        .filter(|other| valid_rect(**other))
        .map(|other| PatchBlocker {
            left: other.x - width,
            right: other.x + other.width,
            top: other.y - height,
            bottom: other.y + other.height,
        })
        .collect();
    let (item_x, item_y) = position_domain(item, body, &blocked, bounds, width, height)?;
    let patch_x = Interval {
        start: item_x.start + offset_x,
        end: item_x.end + offset_x + span_x,
    };
    let patch_y = Interval {
        start: item_y.start + offset_y,
        end: item_y.end + offset_y + span_y,
    };
    let mut candidates = vec![patch_x.start, patch_x.end, body.x, body.x + span_x];
    candidates.extend(blocked.iter().flat_map(|other| [other.left, other.right]));
    candidates.retain(|x| *x >= patch_x.start && *x <= patch_x.end);
    candidates.sort_by(f64::total_cmp);
    candidates.dedup();
    let mut best: Option<Point> = None;
    let mut best_distance = f64::INFINITY;
    // Within a vertical strip the free intervals are constant. The nearest
    // placement is at its edge or at an edge of the original body's patch range.
    for x in candidates {
        let intervals: Vec<_> = blocked
            .iter()
            .filter(|other| x > other.left && x < other.right)
            .map(|other| Interval {
                start: other.top,
                end: other.bottom,
            })
            .collect();
        for free in free_intervals(intervals, patch_y) {
            let y = body.y.clamp(free.start, free.end);
            let position = Point {
                x: item
                    .x
                    .clamp(x - offset_x - span_x, x - offset_x)
                    .clamp(item_x.start, item_x.end),
                y: item
                    .y
                    .clamp(y - offset_y - span_y, y - offset_y)
                    .clamp(item_y.start, item_y.end),
            };
            let distance = (position.x - item.x).powi(2) + (position.y - item.y).powi(2);
            if distance < best_distance - EPSILON
                || ((distance - best_distance).abs() <= EPSILON
                    && best.is_none_or(|best| {
                        position.y < best.y || (position.y == best.y && position.x < best.x)
                    }))
            {
                best = Some(position);
                best_distance = distance;
            }
        }
    }
    best
}

fn position_domain(
    item: Rect,
    body: Rect,
    blocked: &[PatchBlocker],
    bounds: Option<Rect>,
    width: f64,
    height: f64,
) -> Option<(Interval, Interval)> {
    if let Some(bounds) = bounds {
        if !valid_rect(bounds) || bounds.width < item.width || bounds.height < item.height {
            return None;
        }
        return Some((
            Interval {
                start: bounds.x,
                end: bounds.x + (bounds.width - item.width),
            },
            Interval {
                start: bounds.y,
                end: bounds.y + (bounds.height - item.height),
            },
        ));
    }
    // A finite search domain is sufficient on the infinite plane: beyond the
    // outermost expanded blocker edge, a patch can move inward to that edge
    // without hitting anything. Including the original patch range ensures
    // this projection never increases movement. Convert that envelope to the
    // full range of item positions that can contain one of those patches.
    let mut x = Interval {
        start: body.x,
        end: body.x + (body.width - width),
    };
    let mut y = Interval {
        start: body.y,
        end: body.y + (body.height - height),
    };
    for other in blocked {
        x.start = x.start.min(other.left);
        x.end = x.end.max(other.right);
        y.start = y.start.min(other.top);
        y.end = y.end.max(other.bottom);
    }
    let offset_x = body.x - item.x;
    let offset_y = body.y - item.y;
    Some((
        Interval {
            start: x.start - offset_x - (body.width - width),
            end: x.end - offset_x,
        },
        Interval {
            start: y.start - offset_y - (body.height - height),
            end: y.end - offset_y,
        },
    ))
}

fn free_intervals(mut blocked: Vec<Interval>, range: Interval) -> Vec<Interval> {
    blocked.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut free = Vec::new();
    let mut cursor = range.start;
    for interval in blocked {
        if interval.end < cursor {
            continue;
        }
        if interval.start >= cursor {
            free.push(Interval {
                start: cursor,
                end: range.end.min(interval.start),
            });
        }
        cursor = cursor.max(interval.end);
        if cursor > range.end {
            break;
        }
    }
    if cursor <= range.end {
        free.push(Interval {
            start: cursor,
            end: range.end,
        });
    }
    free
}

fn valid_rect(rect: Rect) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite()
        && rect.width > 0.0
        && rect.height > 0.0
        && (rect.x + rect.width).is_finite()
        && (rect.y + rect.height).is_finite()
}
