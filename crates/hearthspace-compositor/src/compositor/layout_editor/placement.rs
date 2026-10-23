//! Explicit placement changes preserve canonical homes and retarget presentation motion.
use super::{App, ObjectPlacement, Target, TargetKind};
impl App {
    pub(super) fn place_layout_target(&mut self, target: &Target, mut placement: ObjectPlacement) {
        if let Some(index) = self.target_window_index(target) {
            self.forget_canvas_peek_home(index);
        }
        if matches!(target.target.kind, TargetKind::Widget | TargetKind::Group) {
            let current = self
                .target_window_index(target)
                .and_then(|index| self.layout_motion_rect(index))
                .or_else(|| self.frame_placed(target).map(|p| p.rect));
            if let Some(current) = current {
                let point = crate::geometry::screen_to_canvas(
                    (current.x, current.y).into(),
                    self.viewport_offset,
                    self.viewport_scale,
                );
                if let ObjectPlacement::Canvas { x, y, .. }
                | ObjectPlacement::Background { x, y, .. } = &mut placement
                {
                    *x = x.or(Some(point.x));
                    *y = y.or(Some(point.y));
                }
            }
        }
        let old_target_rect = self
            .target_window_index(target)
            .and_then(|index| self.layout_motion_rect(index))
            .or_else(|| self.frame_placed(target).map(|p| p.rect));
        let old_placement = self.target_placement(target);
        let old_vertical = self.frame_placed(target).map_or_else(
            || old_placement.edge().is_some_and(|edge| edge.is_vertical()),
            |p| p.vertical,
        );
        let turning = super::turns_as_toolbar(&target.target)
            && old_vertical != placement.edge().is_some_and(|edge| edge.is_vertical());
        let animate_turn = turning || self.layout_target_turn_active(target);
        if matches!(target.target.kind, TargetKind::Window | TargetKind::Item)
            && let Some((Some(x), Some(y), _, _)) = placement.canvas_bounds()
            && let Some(index) = self.target_window_index(target)
        {
            let rect = self.window_canvas_rect(index);
            let old = self.windows[index].position;
            self.windows[index].position = crate::geometry::CanvasPoint {
                x: x.round() as i32 - (rect.loc.x - old.x),
                y: y.round() as i32 - (rect.loc.y - old.y),
            };
            self.windows[index].pending_resize = None;
        }
        let before: Vec<_> = self
            .layout_editor
            .placed
            .iter()
            .filter_map(|p| {
                self.target_window_index(&p.target).map(|i| {
                    (
                        self.windows[i].id,
                        self.layout_motion_rect(i)
                            .or_else(|| self.layout_target_rect(i))
                            .unwrap_or(p.rect),
                    )
                })
            })
            .collect();
        if matches!(target.target.kind, TargetKind::Window | TargetKind::Item) {
            for (output, layout) in &mut self.layout_editor.layouts {
                if *output != target.output {
                    layout.remove_target(&target.target);
                }
            }
        }
        self.layout_editor
            .layouts
            .get_mut(&target.output)
            .unwrap()
            .set_placement(&target.target, placement);
        self.layout_editor.placed = self.frame_placements();
        if self.settle_frame_overlaps() {
            self.layout_editor.placed = self.frame_placements();
        }
        let mut turn_members = Vec::new();
        if animate_turn && let Some(placed) = self.frame_placed(target).cloned() {
            let members = self.layout_turn_members(target);
            let (local_pivot, anchor) =
                if let Some((pointer, old, vertical)) = self.layout_editor.drop_anchor {
                    (
                        super::grip::ToolbarGrip::at(old, pointer, vertical).pivot(
                            placed.rect.width,
                            placed.rect.height,
                            placed.vertical,
                        ),
                        super::Point {
                            x: self.pointer_location.x,
                            y: self.pointer_location.y,
                        },
                    )
                } else {
                    let from = old_target_rect.unwrap_or(placed.rect);
                    (
                        super::Point {
                            x: placed.rect.width / 2.,
                            y: placed.rect.height / 2.,
                        },
                        super::Point {
                            x: from.x + from.width / 2.,
                            y: from.y + from.height / 2.,
                        },
                    )
                };
            let pivot = super::Point {
                x: placed.rect.x + local_pivot.x,
                y: placed.rect.y + local_pivot.y,
            };
            let translation = super::Point {
                x: anchor.x - pivot.x,
                y: anchor.y - pivot.y,
            };
            turn_members = members.clone();
            self.queue_layout_turn(
                target.clone(),
                members,
                placed.rect,
                pivot,
                if !turning {
                    0.
                } else if old_vertical {
                    std::f64::consts::FRAC_PI_2
                } else {
                    -std::f64::consts::FRAC_PI_2
                },
                translation,
            );
        }
        for (id, from) in before {
            if turn_members.contains(&id) {
                continue;
            }
            if let Some(index) = self.windows.iter().position(|w| w.id == id)
                && let Some(to) = self.layout_target_rect(index)
            {
                self.queue_layout_motion(
                    id,
                    from,
                    to,
                    if self.is_layout_surface(index) {
                        super::motion::WIDGET_DURATION_MS
                    } else {
                        super::motion::PLACEMENT_DURATION_MS
                    },
                );
            }
        }
        self.canvas_peek_drop_for_target(target);
    }
}
