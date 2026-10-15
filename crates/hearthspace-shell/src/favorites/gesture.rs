use serde::{Deserialize, Serialize};

use super::FavoriteError;

pub type Point = [f64; 2];
pub const MAX_STROKE_POINTS: usize = 4096;
const SAMPLE_COUNT: usize = 64;
const MAX_ROTATION: f64 = 20.0 * std::f64::consts::PI / 180.0;
pub(super) const MATCH_DISTANCE: f64 = 0.105;
pub(super) const AMBIGUITY_MARGIN: f64 = 0.035;

/// A bounded single stroke, centered and uniformly scaled to a unit bounding box.
///
/// Equidistant resampling follows the geometric template approach described by
/// Wobbrock, Wilson and Li's $1 recognizer (https://depts.washington.edu/acelab/proj/dollar/).
/// We preserve aspect ratio and orientation, limit rotational tolerance, and reject
/// uncertain matches instead of always choosing the nearest template.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "StoredGesture")]
pub struct Gesture {
    points: Vec<Point>,
    closed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredGesture {
    points: Vec<Point>,
    closed: bool,
}

impl Gesture {
    pub fn from_points(points: &[Point]) -> Result<Self, FavoriteError> {
        if !(2..=MAX_STROKE_POINTS).contains(&points.len()) {
            return Err(invalid(
                "Draw one continuous shape with fewer than 4096 points.",
            ));
        }
        if points
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
        {
            return Err(invalid("The shape contains invalid coordinates."));
        }
        let extent = extent(points);
        let length = path_length(points);
        if extent < 24.0 || length < 40.0 {
            return Err(invalid(
                "Draw a larger shape, rather than a tap or a tiny stroke.",
            ));
        }
        if length / extent > 20.0 {
            return Err(invalid("Use a simpler single-stroke shape."));
        }
        let closed = distance(points[0], points[points.len() - 1]) <= extent * 0.18
            && length >= extent * 2.0;
        let mut source = points.to_vec();
        if closed {
            source.push(points[0]);
        }
        let mut points = resample(&source, closed);
        normalize(&mut points);
        Ok(Self { points, closed })
    }

    pub fn points(&self) -> &[Point] {
        &self.points
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Mean point distance after bounded rotation; closed paths also try shifted
    /// starts. Reversing a drawing does not assign a different app to the same shape.
    pub(super) fn distance_to(&self, other: &Self) -> f64 {
        if self.closed != other.closed {
            return f64::INFINITY;
        }
        let shifts = if self.closed { SAMPLE_COUNT } else { 1 };
        let mut best = f64::INFINITY;
        for reverse in [false, true] {
            for shift in 0..shifts {
                let index = |i: usize| {
                    if reverse {
                        (SAMPLE_COUNT - 1 - i + shift) % SAMPLE_COUNT
                    } else {
                        (i + shift) % SAMPLE_COUNT
                    }
                };
                let (mut dot, mut cross) = (0.0, 0.0);
                for (i, a) in self.points.iter().enumerate() {
                    let b = other.points[index(i)];
                    dot += a[0] * b[0] + a[1] * b[1];
                    cross += a[0] * b[1] - a[1] * b[0];
                }
                let angle = cross.atan2(dot).clamp(-MAX_ROTATION, MAX_ROTATION);
                let (sin, cos) = angle.sin_cos();
                let error: f64 = self
                    .points
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        let rotated = [a[0] * cos - a[1] * sin, a[0] * sin + a[1] * cos];
                        distance(rotated, other.points[index(i)])
                    })
                    .sum();
                best = best.min(error / SAMPLE_COUNT as f64);
            }
        }
        best
    }
}

impl TryFrom<StoredGesture> for Gesture {
    type Error = FavoriteError;

    fn try_from(value: StoredGesture) -> Result<Self, Self::Error> {
        if value.points.len() != SAMPLE_COUNT
            || value
                .points
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 1.0)
            || (extent(&value.points) - 1.0).abs() > 0.001
            || centroid(&value.points).iter().any(|v| v.abs() > 0.001)
            || path_length(&value.points) > 20.0
            || (value.closed && distance(value.points[0], value.points[SAMPLE_COUNT - 1]) > 0.4)
        {
            return Err(invalid("The saved shape is malformed."));
        }
        Ok(Self {
            points: value.points,
            closed: value.closed,
        })
    }
}

fn resample(points: &[Point], closed: bool) -> Vec<Point> {
    let count = if closed {
        SAMPLE_COUNT
    } else {
        SAMPLE_COUNT - 1
    };
    let step = path_length(points) / count as f64;
    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    let (mut segment, mut covered) = (1, 0.0);
    for sample in 0..SAMPLE_COUNT {
        let target = sample as f64 * step;
        while segment < points.len() - 1
            && covered + distance(points[segment - 1], points[segment]) < target
        {
            covered += distance(points[segment - 1], points[segment]);
            segment += 1;
        }
        let from = points[segment - 1];
        let to = points[segment];
        let length = distance(from, to);
        let ratio = if length > 0.0 {
            ((target - covered) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        samples.push([
            from[0] + (to[0] - from[0]) * ratio,
            from[1] + (to[1] - from[1]) * ratio,
        ]);
    }
    samples
}

fn normalize(points: &mut [Point]) {
    let scale = extent(points);
    let center = centroid(points);
    for point in points {
        point[0] = (point[0] - center[0]) / scale;
        point[1] = (point[1] - center[1]) / scale;
    }
}

fn centroid(points: &[Point]) -> Point {
    let mut center = [0.0, 0.0];
    for point in points {
        center[0] += point[0];
        center[1] += point[1];
    }
    [
        center[0] / points.len() as f64,
        center[1] / points.len() as f64,
    ]
}

fn extent(points: &[Point]) -> f64 {
    let (mut min, mut max) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for point in points {
        for i in 0..2 {
            min[i] = min[i].min(point[i]);
            max[i] = max[i].max(point[i]);
        }
    }
    (max[0] - min[0]).max(max[1] - min[1])
}

fn path_length(points: &[Point]) -> f64 {
    points
        .windows(2)
        .map(|pair| distance(pair[0], pair[1]))
        .sum()
}

fn distance(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn invalid(message: &str) -> FavoriteError {
    FavoriteError(message.to_owned())
}

#[cfg(test)]
#[path = "gesture_tests.rs"]
mod tests;
