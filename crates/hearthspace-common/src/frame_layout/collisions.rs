//! Merge attachment collisions using pre-merge geometry, so changes in group width
//! cannot accidentally absorb additional objects during the same layout pass.
use std::collections::BTreeSet;

use super::{FrameLayout, LayoutTarget, Rect, TargetKind};

pub fn merge_overlapping_frame_items(
    layout: &mut FrameLayout,
    entries: &[(LayoutTarget, Rect)],
) -> bool {
    let eligible: Vec<_> = entries
        .iter()
        .filter(|(target, bounds)| {
            (target.kind != TargetKind::Group || layout.groups.contains_key(&target.id))
                && layout.placement(target).is_frame()
                && bounds.width > 0.0
                && bounds.height > 0.0
                && [bounds.x, bounds.y, bounds.width, bounds.height]
                    .into_iter()
                    .all(f64::is_finite)
        })
        .collect();
    let listed_groups: BTreeSet<_> = eligible
        .iter()
        .filter_map(|(target, _)| (target.kind == TargetKind::Group).then_some(target.id.as_str()))
        .collect();
    let mut seen = BTreeSet::new();
    let attachments: Vec<_> = eligible
        .into_iter()
        .filter(|(target, _)| {
            !(target.kind != TargetKind::Group
                && layout
                    .group_for(target)
                    .is_some_and(|group| listed_groups.contains(group.id.as_str())))
                && seen.insert(target.key())
        })
        .collect();
    let mut roots: Vec<_> = (0..attachments.len()).collect();
    for first in 0..attachments.len() {
        for second in first + 1..attachments.len() {
            if overlaps(attachments[first].1, attachments[second].1) {
                let first = root(&roots, first);
                let second = root(&roots, second);
                roots[first.max(second)] = first.min(second);
            }
        }
    }
    let mut changed = false;
    for index in 0..attachments.len() {
        let destination = root(&roots, index);
        if destination == index {
            continue;
        }
        let target = &attachments[destination].0;
        let target = layout
            .group_for(target)
            .map(|group| LayoutTarget::new(TargetKind::Group, &group.id))
            .unwrap_or_else(|| target.clone());
        changed |= layout
            .merge(&attachments[index].0, &target, false)
            .is_some();
    }
    changed
}

fn root(roots: &[usize], mut index: usize) -> usize {
    while roots[index] != index {
        index = roots[index];
    }
    index
}

fn overlaps(first: Rect, second: Rect) -> bool {
    first.x < second.x + second.width
        && first.x + first.width > second.x
        && first.y < second.y + second.height
        && first.y + first.height > second.y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_layout::{FrameEdge, ObjectPlacement};

    #[test]
    fn viewport_collision_merges_connected_attachments_once_in_measured_order() {
        let mut layout = FrameLayout::default();
        let first = LayoutTarget::widget("notifications");
        let second = LayoutTarget::widget("apps");
        let third = LayoutTarget::widget("tools");
        for target in [&first, &second, &third] {
            layout.set_placement(target, ObjectPlacement::frame(FrameEdge::Top, 0.0));
        }
        let bounds = |x| Rect {
            x,
            y: 24.0,
            width: 100.0,
            height: 54.0,
        };
        assert!(merge_overlapping_frame_items(
            &mut layout,
            &[
                (first.clone(), bounds(24.0)),
                (second.clone(), bounds(110.0)),
                (third.clone(), bounds(200.0))
            ]
        ));
        assert_eq!(
            layout.group_for(&first).unwrap().members,
            [first.clone(), second, third]
        );
        assert_eq!(layout.validate(), Ok(()));
        let group = LayoutTarget::new(TargetKind::Group, &layout.group_for(&first).unwrap().id);
        assert!(!merge_overlapping_frame_items(
            &mut layout,
            &[(group, bounds(24.0)), (first, bounds(24.0))]
        ));
    }

    #[test]
    fn touching_canvas_hidden_and_invalid_bounds_never_merge() {
        let mut layout = FrameLayout::default();
        let first = LayoutTarget::widget("notifications");
        let second = LayoutTarget::widget("apps");
        let tools = LayoutTarget::widget("tools");
        layout.set_placement(&tools, ObjectPlacement::canvas());
        let bounds = |x| Rect {
            x,
            y: 24.0,
            width: 100.0,
            height: 54.0,
        };
        assert!(!merge_overlapping_frame_items(
            &mut layout,
            &[
                (first.clone(), bounds(24.0)),
                (second.clone(), bounds(124.0)),
                (tools, bounds(24.0))
            ]
        ));
        layout.set_placement(&first, ObjectPlacement::Hidden);
        assert!(!merge_overlapping_frame_items(
            &mut layout,
            &[(first, bounds(24.0)), (second, bounds(f64::NAN))]
        ));
    }
}
