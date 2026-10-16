//! Bounded screen-space freeform selection with even-odd polygon semantics.
use std::cell::Cell;

use smithay::utils::{Logical, Point, Rectangle};

type Vertex = Point<f64, Logical>;
const MAX_POINTS: usize = 256;
const COORDINATE_LIMIT: f64 = 1_000_000.0;
const EPSILON: f64 = 1e-7;

pub(super) struct Lasso {
    points: Vec<Vertex>,
    spacing: f64,
    has_area: Cell<Option<bool>>,
}

impl Lasso {
    pub(super) fn new(point: Vertex) -> Self {
        let mut lasso = Self {
            points: Vec::with_capacity(MAX_POINTS),
            spacing: 3.0,
            has_area: Cell::new(None),
        };
        lasso.push(point);
        lasso
    }

    /// Retain the first point and live endpoint, sampling intermediate points.
    /// Long gestures progressively coarsen instead of growing without bound.
    pub(super) fn push(&mut self, point: Vertex) {
        if !point.x.is_finite() || !point.y.is_finite() {
            return;
        }
        let point = Vertex::from((
            point.x.clamp(-COORDINATE_LIMIT, COORDINATE_LIMIT),
            point.y.clamp(-COORDINATE_LIMIT, COORDINATE_LIMIT),
        ));
        if self.points.last() == Some(&point) {
            return;
        }
        self.has_area.set(None);
        let len = self.points.len();
        if len >= 2 && distance(self.points[len - 2], self.points[len - 1]) < self.spacing {
            self.points[len - 1] = point;
            return;
        }
        if len == MAX_POINTS {
            let last = self.points[len - 1];
            let mut index = 0;
            self.points.retain(|_| {
                let keep = index % 2 == 0;
                index += 1;
                keep
            });
            self.points.push(last);
            self.spacing *= 2.0;
        }
        self.points.push(point);
    }

    pub(super) fn points(&self) -> &[Vertex] {
        &self.points
    }

    pub(super) fn selects(&self, rect: Rectangle<i32, Logical>) -> bool {
        if rect.size.w <= 0 || rect.size.h <= 0 || !self.has_area() {
            return false;
        }
        let left = f64::from(rect.loc.x);
        let top = f64::from(rect.loc.y);
        let right = left + f64::from(rect.size.w);
        let bottom = top + f64::from(rect.size.h);
        let corners = [
            Vertex::from((left, top)),
            Vertex::from((right, top)),
            Vertex::from((right, bottom)),
            Vertex::from((left, bottom)),
        ];
        self.points
            .iter()
            .any(|p| p.x >= left && p.x <= right && p.y >= top && p.y <= bottom)
            || corners.iter().any(|&p| contains(&self.points, p))
            || edges(&self.points)
                .any(|(a, b)| edges(&corners).any(|(c, d)| segments_intersect(a, b, c, d)))
    }

    fn has_area(&self) -> bool {
        if let Some(value) = self.has_area.get() {
            return value;
        }
        let value = polygon_has_area(&self.points);
        self.has_area.set(Some(value));
        value
    }
}

fn distance(a: Vertex, b: Vertex) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

fn edges(points: &[Vertex]) -> impl Iterator<Item = (Vertex, Vertex)> + '_ {
    points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
}

fn cross(a: Vertex, b: Vertex, c: Vertex) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn on_segment(a: Vertex, b: Vertex, p: Vertex) -> bool {
    cross(a, b, p).abs() < EPSILON
        && p.x >= a.x.min(b.x) - EPSILON
        && p.x <= a.x.max(b.x) + EPSILON
        && p.y >= a.y.min(b.y) - EPSILON
        && p.y <= a.y.max(b.y) + EPSILON
}

fn segments_intersect(a: Vertex, b: Vertex, c: Vertex, d: Vertex) -> bool {
    let signs = (
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    );
    (signs.0 * signs.1 < 0.0 && signs.2 * signs.3 < 0.0)
        || on_segment(a, b, c)
        || on_segment(a, b, d)
        || on_segment(c, d, a)
        || on_segment(c, d, b)
}

fn contains(points: &[Vertex], point: Vertex) -> bool {
    let mut inside = false;
    for (a, b) in edges(points) {
        if on_segment(a, b, point) {
            return true;
        }
        if (a.y > point.y) != (b.y > point.y)
            && point.x < a.x + (point.y - a.y) * (b.x - a.x) / (b.y - a.y)
        {
            inside = !inside;
        }
    }
    inside
}

/// Integrate horizontal even-odd spans between vertices and crossings. This
/// rejects retraced/collinear paths while accepting self-crossing freeform loops.
fn polygon_has_area(points: &[Vertex]) -> bool {
    if points.len() < 3 {
        return false;
    }
    let segments: Vec<_> = edges(points).collect();
    let mut levels: Vec<_> = points.iter().map(|point| point.y).collect();
    for (index, &(a, b)) in segments.iter().enumerate() {
        for &(c, d) in &segments[index + 1..] {
            let denominator = (b.x - a.x) * (d.y - c.y) - (b.y - a.y) * (d.x - c.x);
            if denominator.abs() < EPSILON {
                continue;
            }
            let t = ((c.x - a.x) * (d.y - c.y) - (c.y - a.y) * (d.x - c.x)) / denominator;
            let u = ((c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x)) / denominator;
            if (0.0..1.0).contains(&t) && (0.0..1.0).contains(&u) {
                levels.push(a.y + t * (b.y - a.y));
            }
        }
    }
    levels.sort_unstable_by(f64::total_cmp);
    levels.dedup_by(|a, b| (*a - *b).abs() < EPSILON);
    let mut area = 0.0;
    let mut crossings = Vec::with_capacity(points.len());
    for pair in levels.windows(2) {
        let y = (pair[0] + pair[1]) / 2.0;
        crossings.clear();
        crossings.extend(
            segments
                .iter()
                .filter(|&&(a, b)| (a.y > y) != (b.y > y))
                .map(|&(a, b)| a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y)),
        );
        crossings.sort_unstable_by(f64::total_cmp);
        area += crossings
            .as_chunks::<2>()
            .0
            .iter()
            .map(|span| span[1] - span[0])
            .sum::<f64>()
            * (pair[1] - pair[0]);
        if area >= 1.0 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
