//! Deterministic rectangle peeking in canvas coordinates, independent of presentation.
//!
//! Callers provide only participating canvas objects, with windows already ordered across
//! their layers. Frame attachments and hidden objects belong outside this scene. Window
//! movement is permanent; decorations retain their first home until it is clear again.

mod geometry;
mod restore;

pub use geometry::{fully_covered, reveal_position};
pub use restore::restore_background;

use crate::frame_layout::{Point, Rect};

pub const WINDOW_PEEK: f64 = 24.0;
pub const DECORATION_PEEK: f64 = 32.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub id: u64,
    pub bounds: Rect,
    pub background: bool,
    /// Fixed windows still occlude lower windows and decorations.
    pub fixed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decoration {
    pub id: u64,
    /// Complete object bounds, including its label and padding.
    pub bounds: Rect,
    /// Opaque artwork in canvas coordinates. Use the full bounds for controls.
    pub body: Rect,
    /// Original bounds position before the first automatic displacement.
    pub home: Option<Point>,
    /// Above background windows/decorations, below ordinary windows.
    pub raised: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obstacle {
    pub bounds: Rect,
    pub background: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    /// Bottom to top; settlement preserves this order and every object's size.
    pub windows: Vec<Window>,
    /// Bottom to top within each layer. Raised decorations take precedence.
    pub decorations: Vec<Decoration>,
    /// Fixed placement footprints, independent of window identity and stacking.
    pub obstacles: Vec<Obstacle>,
}

/// Reveal covered windows, then restore and reveal decorations.
///
/// `None` permits the entire infinite plane, including negative coordinates.
/// `Some(bounds)` constrains new reveal positions to fit the entire object in
/// those bounds. Partly exposed objects and impossible placements stay put.
pub fn settle(scene: &mut Scene, bounds: Option<Rect>) {
    let mut higher: Vec<_> = scene.obstacles.iter().map(|item| item.bounds).collect();
    for window in scene.windows.iter_mut().rev() {
        if !window.fixed
            && fully_covered(window.bounds, &higher)
            && let Some(position) =
                reveal_position(window.bounds, window.bounds, &higher, bounds, WINDOW_PEEK)
        {
            window.bounds.x = position.x;
            window.bounds.y = position.y;
        }
        higher.push(window.bounds);
    }
    settle_background(scene, bounds);
}

/// Restore and reveal decorations without moving windows.
///
/// This also retains temporary homes. A caller applying a permanent background
/// drop can adopt the affected resulting positions separately.
pub fn settle_background(scene: &mut Scene, bounds: Option<Rect>) {
    restore_background(scene);
    settle_decorations(scene, bounds, true);
}

/// Reconsider existing peeks after removals without displacing new objects.
pub fn relax_background(scene: &mut Scene) {
    restore_background(scene);
    settle_decorations(scene, None, false);
}

fn settle_decorations(scene: &mut Scene, bounds: Option<Rect>, allow_new: bool) {
    let mut higher: Vec<Decoration> = Vec::with_capacity(scene.decorations.len());
    for raised in [true, false] {
        let windows = covering_windows(scene, raised);
        for item in scene
            .decorations
            .iter_mut()
            .rev()
            .filter(|item| item.raised == raised)
        {
            let mut blockers = windows.clone();
            blockers.extend(
                higher
                    .iter()
                    .filter(|item| !raised || item.raised)
                    .map(|item| item.body),
            );
            let covered = fully_covered(item.body, &blockers);
            let position = if item.home.is_some() || (allow_new && covered) {
                decoration_position(*item, &blockers, bounds).or_else(|| {
                    // A lower peer can prevent restoration without occluding
                    // this layer. Preserve discovery if home-based relaxation
                    // has no admissible candidate but the current pose is hidden.
                    covered
                        .then(|| {
                            reveal_position(
                                item.bounds,
                                item.body,
                                &blockers,
                                bounds,
                                DECORATION_PEEK,
                            )
                        })
                        .flatten()
                })
            } else {
                None
            };
            if let Some(position) = position
                && (covered || closer_to_home(*item, position))
                && (position.x != item.bounds.x || position.y != item.bounds.y)
            {
                item.home.get_or_insert(Point {
                    x: item.bounds.x,
                    y: item.bounds.y,
                });
                *item = item.at(position);
            }
            higher.push(*item);
        }
    }
}

fn decoration_position(item: Decoration, blockers: &[Rect], bounds: Option<Rect>) -> Option<Point> {
    let origin = item.home.map_or(item, |home| item.at(home));
    let mut position = reveal_position(
        origin.bounds,
        origin.body,
        blockers,
        bounds,
        DECORATION_PEEK,
    )?;
    if item.home == Some(position) {
        // Restoration already rejected this home (e.g. a window still covers
        // its label). A zero-distance artwork patch must not bypass that rule.
        // Seek the nearest completely clear tile instead of returning under it.
        position = reveal_position(
            origin.bounds,
            origin.bounds,
            blockers,
            bounds,
            origin.bounds.width.max(origin.bounds.height),
        )?;
        if item.home == Some(position) {
            return None;
        }
    }
    Some(position)
}

fn closer_to_home(item: Decoration, position: Point) -> bool {
    item.home.is_some_and(|home| {
        let distance = |p: Point| (p.x - home.x).powi(2) + (p.y - home.y).powi(2);
        distance(position) + 0.000_001
            < distance(Point {
                x: item.bounds.x,
                y: item.bounds.y,
            })
    })
}

/// The reference desktop icon's centered 56px artwork, above its 20px label area.
pub fn icon_body(bounds: Rect) -> Rect {
    let size = 56.0_f64.min(bounds.width).min(bounds.height).max(0.0);
    Rect {
        x: bounds.x + (bounds.width - size) / 2.0,
        y: bounds.y + ((bounds.height - size - 20.0) / 2.0).max(0.0),
        width: size,
        height: size,
    }
}

impl Decoration {
    fn at(mut self, position: Point) -> Self {
        self.body.x += position.x - self.bounds.x;
        self.body.y += position.y - self.bounds.y;
        self.bounds.x = position.x;
        self.bounds.y = position.y;
        self
    }
}

fn covering_windows(scene: &Scene, raised: bool) -> Vec<Rect> {
    scene
        .windows
        .iter()
        .filter(|item| !raised || !item.background)
        .map(|item| item.bounds)
        .chain(
            scene
                .obstacles
                .iter()
                .filter(|item| !raised || !item.background)
                .map(|item| item.bounds),
        )
        .collect()
}

#[cfg(test)]
mod decoration_tests;
#[cfg(test)]
mod restore_tests;
#[cfg(test)]
mod tests;
