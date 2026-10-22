use super::{Decoration, Scene, covering_windows, geometry::intersect};
use crate::frame_layout::Rect;

/// Return displaced decorations to clear homes, without adding any new peeks.
///
/// Windows and obstacles must clear the complete home bounds. Decorations only
/// block one another's artwork. Homes are restored exactly, without clamping.
pub fn restore_background(scene: &mut Scene) {
    let windows = [
        covering_windows(scene, false),
        covering_windows(scene, true),
    ];
    let items = &mut scene.decorations;
    loop {
        let homes: Vec<_> = items
            .iter()
            .map(|item| {
                let mut home = item.home.map_or(*item, |position| item.at(position));
                home.home = None;
                home
            })
            .collect();
        let mut returning: Vec<_> = items.iter().map(|item| item.home.is_some()).collect();
        loop {
            let mut blocked = false;
            // Ascending indices deliberately match the reference's insertion-ordered set.
            for (index, home) in homes.iter().enumerate() {
                if returning[index]
                    && home_blocked(
                        *home,
                        &windows[usize::from(home.raised)],
                        items
                            .iter()
                            .enumerate()
                            .filter(|(other, _)| *other != index)
                            .map(|(other, item)| {
                                if returning[other] {
                                    homes[other]
                                } else {
                                    *item
                                }
                            }),
                    )
                {
                    returning[index] = false;
                    blocked = true;
                }
            }
            // Eliminating a return leaves that peer at its current position,
            // which may block another home. Resolve those dependencies first.
            if !blocked {
                break;
            }
        }
        let mut restored = false;
        for (index, returns) in returning.into_iter().enumerate() {
            if returns {
                items[index] = homes[index];
                restored = true;
            }
        }
        // Cyclic returns above are simultaneous. Also permit an individual to
        // return when a peer's intended home conflicts but its actual spot does not.
        for index in (0..items.len()).rev() {
            let item = items[index];
            if item.home.is_some()
                && !home_blocked(
                    homes[index],
                    &windows[usize::from(item.raised)],
                    items
                        .iter()
                        .enumerate()
                        .filter(|(other, _)| *other != index)
                        .map(|(_, item)| *item),
                )
            {
                items[index] = homes[index];
                restored = true;
            }
        }
        // Each successful return removes one home, so this always converges.
        if !restored {
            break;
        }
    }
}

fn home_blocked(
    home: Decoration,
    windows: &[Rect],
    peers: impl Iterator<Item = Decoration>,
) -> bool {
    windows
        .iter()
        .any(|other| intersect(home.bounds, *other).is_some())
        || peers
            .filter(|other| !home.raised || other.raised)
            .any(|other| intersect(home.body, other.body).is_some())
}
