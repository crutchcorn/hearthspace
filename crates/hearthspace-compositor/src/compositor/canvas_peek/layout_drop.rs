//! A deliberate layout placement acts as an obstacle without moving foreground apps.
use hearthspace_common::{
    canvas_peek::{self, Obstacle, Scene},
    frame_layout::Rect,
};

pub(super) fn settle(scene: &mut Scene, id: u64) {
    let window = scene.windows.iter().find(|w| w.id == id).cloned();
    let decoration = scene.decorations.iter().find(|d| d.id == id).cloned();
    let Some((bounds, background)) = window
        .as_ref()
        .map(|w| (w.bounds, w.background))
        .or_else(|| decoration.as_ref().map(|d| (d.bounds, !d.raised)))
    else {
        canvas_peek::settle(scene, None);
        return;
    };
    let before = scene.clone();
    scene.windows.retain(|w| w.id != id);
    scene.decorations.retain(|d| d.id != id);
    for w in &mut scene.windows {
        w.fixed = !w.background && (background || decoration.is_some());
    }
    scene.obstacles.push(Obstacle { bounds, background });
    canvas_peek::settle(scene, None);
    scene.obstacles.clear();
    for d in &mut scene.decorations {
        let original = before
            .decorations
            .iter()
            .find(|old| old.id == d.id)
            .unwrap();
        if background
            && d.home.is_some_and(|home| {
                original.bounds.x != d.bounds.x
                    || original.bounds.y != d.bounds.y
                    || overlaps(
                        Rect {
                            x: home.x,
                            y: home.y,
                            ..d.bounds
                        },
                        bounds,
                    )
            })
        {
            d.home = None;
        }
    }
    if let Some(window) = window {
        let index = before.windows.iter().position(|w| w.id == id).unwrap();
        scene.windows.insert(index, window);
    }
    if let Some(mut decoration) = decoration {
        decoration.home = None;
        let index = before.decorations.iter().position(|d| d.id == id).unwrap();
        scene.decorations.insert(index, decoration);
        // A dropped decoration must itself remain discoverable under real apps.
        canvas_peek::settle_background(scene, None);
    }
    for w in &mut scene.windows {
        w.fixed = false;
    }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}
