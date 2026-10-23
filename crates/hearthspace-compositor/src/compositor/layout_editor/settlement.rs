//! Preserve ordered parts when splitting, and merge only genuine frame intersections.
use super::{App, LayoutTarget, ObjectPlacement, Target, TargetKind, compact};
use hearthspace_common::frame_layout::{self, Rect};
impl App {
    pub(super) fn settle_frame_overlaps(&mut self) -> bool {
        if self.layout_editor.drag.is_some() {
            return false;
        }
        let mut changed = false;
        for output in self
            .layout_editor
            .layouts
            .keys()
            .cloned()
            .collect::<Vec<_>>()
        {
            let entries: Vec<_> = self
                .layout_editor
                .placed
                .iter()
                .filter(|p| {
                    p.target.output == output
                        && p.placement.is_frame()
                        && !compact::backing(&p.target.target.id)
                })
                .map(|p| {
                    (
                        p.target.target.clone(),
                        if p.target.target.kind == TargetKind::Window {
                            self.pinned_reveal_rect(p)
                        } else {
                            p.rect
                        },
                    )
                })
                .collect();
            changed |= frame_layout::merge_overlapping_frame_items(
                self.layout_editor.layouts.get_mut(&output).unwrap(),
                &entries,
            );
        }
        changed
    }
    pub(super) fn separate_frame_target(&mut self, target: &Target) {
        let Some(group) = self.layout_editor.layouts[&target.output]
            .group_for(&target.target)
            .cloned()
        else {
            return;
        };
        let group_target = Target {
            output: target.output.clone(),
            target: LayoutTarget::new(TargetKind::Group, &group.id),
        };
        let Some(group_rect) = self.frame_placed(&group_target).map(|p| p.rect) else {
            return;
        };
        let placement = self.target_placement(target);
        let Some(edge) = placement.edge() else {
            return;
        };
        let Some(view) = self.frame_output_rect(&target.output) else {
            return;
        };
        let members = if target.target.kind == TargetKind::Group {
            group.members
        } else {
            vec![target.target.clone()]
        };
        let measured: Vec<_> = members
            .iter()
            .map(|member| {
                let member_target = Target {
                    output: target.output.clone(),
                    target: member.clone(),
                };
                let mut rect = self.frame_placed(&member_target).map_or(
                    Rect {
                        width: 54.,
                        height: 54.,
                        ..Default::default()
                    },
                    |p| p.rect,
                );
                if matches!(member.kind, TargetKind::Widget | TargetKind::Item) {
                    rect.width += 10.;
                    rect.height += 10.;
                }
                (member.clone(), rect)
            })
            .collect();
        let total = measured
            .iter()
            .map(|(_, rect)| {
                if edge.is_vertical() {
                    rect.height
                } else {
                    rect.width
                }
            })
            .sum::<f64>()
            + measured.len().saturating_sub(1) as f64 * 12.;
        let margin = if view.width <= 760. { 12. } else { 24. };
        let start = if edge.is_vertical() {
            group_rect.y
        } else {
            group_rect.x
        };
        let extent = if edge.is_vertical() {
            group_rect.height
        } else {
            group_rect.width
        };
        let low = if edge.is_vertical() { view.y } else { view.x } + margin;
        let high = if edge.is_vertical() {
            view.y + view.height
        } else {
            view.x + view.width
        } - margin;
        let mut cursor = if target.target.kind == TargetKind::Group {
            start.min(high - total).max(low)
        } else if start + extent + 12. + total <= high {
            start + extent + 12.
        } else {
            (start - total - 12.).max(low)
        };
        let mut placements = Vec::new();
        for (member, mut rect) in measured {
            if edge.is_vertical() {
                rect.y = cursor;
            } else {
                rect.x = cursor;
            }
            placements.push((member, rect));
            cursor += if edge.is_vertical() {
                rect.height
            } else {
                rect.width
            } + 12.;
        }
        self.layout_editor
            .layouts
            .get_mut(&target.output)
            .unwrap()
            .ungroup(&target.target);
        for (member, rect) in placements {
            let first = frame_layout::frame_bounds(
                ObjectPlacement::frame(edge, 0.),
                rect.width,
                rect.height,
                view,
                true,
            );
            let last = frame_layout::frame_bounds(
                ObjectPlacement::frame(edge, 1.),
                rect.width,
                rect.height,
                view,
                true,
            );
            let (value, start, end) = if edge.is_vertical() {
                (rect.y - view.y, first.y, last.y)
            } else {
                (rect.x - view.x, first.x, last.x)
            };
            let position = if end > start {
                ((value - start) / (end - start)).clamp(0., 1.)
            } else {
                0.5
            };
            self.layout_editor
                .layouts
                .get_mut(&target.output)
                .unwrap()
                .set_placement(&member, ObjectPlacement::frame(edge, position));
        }
    }
    pub(super) fn release_canvas_group(&mut self, target: &Target) {
        self.release_layout_group(target, false);
    }
    pub(super) fn release_layout_group(&mut self, target: &Target, background: bool) {
        let Some(group) = self.layout_editor.layouts[&target.output]
            .groups
            .get(&target.target.id)
            .cloned()
        else {
            return;
        };
        let members: Vec<_> = group
            .members
            .iter()
            .filter_map(|member| {
                let target = Target {
                    output: target.output.clone(),
                    target: member.clone(),
                };
                let rect = self.frame_placed(&target)?.rect;
                Some((target, rect))
            })
            .collect();
        self.layout_editor
            .layouts
            .get_mut(&target.output)
            .unwrap()
            .ungroup(&target.target);
        for (target, rect) in members {
            let point = crate::geometry::screen_to_canvas(
                (rect.x, rect.y).into(),
                self.viewport_offset,
                self.viewport_scale,
            );
            self.place_layout_target(
                &target,
                if background {
                    ObjectPlacement::Background {
                        x: Some(point.x),
                        y: Some(point.y),
                        width: None,
                        height: None,
                    }
                } else {
                    ObjectPlacement::Canvas {
                        x: Some(point.x),
                        y: Some(point.y),
                        width: None,
                        height: None,
                    }
                },
            );
        }
    }
}
