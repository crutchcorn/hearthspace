//! Pointer selection uses the same composed toolbar pose that was last displayed.
use super::{App, LayoutTarget, Point, Rect, Target, TargetKind, input::contains, turn::Presented};

fn inverse(point: Point, pivot: Point, angle: f64) -> Point {
    let (sin, cos) = angle.sin_cos();
    let x = point.x - pivot.x;
    let y = point.y - pivot.y;
    Point {
        x: pivot.x + cos * x + sin * y,
        y: pivot.y - sin * x + cos * y,
    }
}

impl App {
    fn layout_pointer_turn(&self, target: &Target) -> Option<(String, Target, Presented)> {
        let id = self
            .target_window_index(target)
            .map(|index| self.windows[index].id);
        let (key, turn) = self.layout_editor.turn.turns.iter().find(|(_, turn)| {
            turn.target == *target || id.is_some_and(|id| turn.members.contains(&id))
        })?;
        let presented = self
            .layout_editor
            .turn
            .presented
            .get(key)
            .cloned()
            .or_else(|| {
                let bounds = self
                    .target_window_index(&turn.target)
                    .and_then(|index| self.layout_motion_rect(index))
                    .or_else(|| self.frame_placed(&turn.target).map(|placed| placed.rect))?;
                Some(Presented {
                    bounds,
                    pivot: Point {
                        x: bounds.x + turn.pivot.x - turn.bounds.x,
                        y: bounds.y + turn.pivot.y - turn.bounds.y,
                    },
                    angle: turn.angle(self.layout_editor.motion.sampled_at),
                    vertical: self.frame_placed(&turn.target)?.vertical,
                    members: turn
                        .members
                        .iter()
                        .filter_map(|id| {
                            let index = self.windows.iter().position(|window| window.id == *id)?;
                            self.layout_motion_rect(index)
                                .or_else(|| self.layout_target_rect(index))
                                .map(|rect| (*id, rect))
                        })
                        .collect(),
                })
            })?;
        Some((key.clone(), turn.target.clone(), presented))
    }

    pub(super) fn interrupt_layout_turn_at(
        &mut self,
        target: &Target,
        point: Point,
    ) -> Option<(Rect, bool)> {
        let (key, owner, presented) = self.layout_pointer_turn(target)?;
        let bounds = if owner == *target {
            presented.bounds
        } else {
            let id = self.windows[self.target_window_index(target)?].id;
            presented
                .members
                .iter()
                .find(|(member, _)| *member == id)?
                .1
        };
        let local = inverse(point, presented.pivot, presented.angle);
        self.layout_editor.turn.turns.remove(&key);
        self.layout_editor.turn.presented.remove(&key);
        self.request_redraw();
        Some((
            Rect {
                x: point.x - (local.x - bounds.x),
                y: point.y - (local.y - bounds.y),
                ..bounds
            },
            presented.vertical,
        ))
    }

    pub(super) fn layout_target_at(&self, point: Point) -> Option<Target> {
        for placed in self.layout_editor.placed.iter().rev() {
            if placed.target.target.kind != TargetKind::Group
                || super::compact::backing(&placed.target.target.id)
            {
                continue;
            }
            let mut rect = self.layout_grip_rect(placed);
            let mut local = point;
            if let Some((_, _, presented)) = self.layout_pointer_turn(&placed.target) {
                local = inverse(point, presented.pivot, presented.angle);
                if let Some((_, captured)) = presented.members.iter().find(|(id, _)| {
                    self.windows
                        .iter()
                        .position(|window| window.id == *id)
                        .is_some_and(|index| self.frame_grip_identity(index).is_some())
                }) {
                    rect = *captured;
                }
            }
            if contains(rect, local) {
                return Some(placed.target.clone());
            }
        }
        let mut indices: Vec<_> = (0..self.windows.len()).collect();
        indices.sort_by_key(|i| {
            (
                self.layout_render_layer(*i)
                    .unwrap_or(if self.desktop_id(*i).is_some() { 1 } else { 10 }),
                *i,
            )
        });
        for index in indices.into_iter().rev() {
            if !self.windows[index].mapped
                || self.compact_frame_backing(index)
                || self.is_layout_surface(index) && !self.layout_surface_visible(index)
            {
                continue;
            }
            let target = self.frame_target_for_window(index).or_else(|| {
                let kind = if self.desktop_id(index).is_some() {
                    TargetKind::Item
                } else if self.is_navigable_window(index) {
                    TargetKind::Window
                } else {
                    return None;
                };
                Some(Target {
                    output: self.layout_editor.session.as_ref()?.output.clone(),
                    target: LayoutTarget::new(
                        kind,
                        self.desktop_id(index)
                            .unwrap_or_else(|| self.windows[index].id.to_string()),
                    ),
                })
            });
            let Some(target) = target else {
                continue;
            };
            let r = self.window_screen_bounds(index);
            let mut rect = self
                .layout_motion_rect(index)
                .or_else(|| {
                    self.frame_placed(&target).map(|placed| {
                        if target.target.kind == TargetKind::Window && placed.placement.is_frame() {
                            self.pinned_reveal_rect(placed)
                        } else {
                            placed.rect
                        }
                    })
                })
                .map(|mut rect| {
                    let offset = self.canvas_peek_offset(index);
                    rect.x += offset.x;
                    rect.y += offset.y;
                    rect
                })
                .unwrap_or(Rect {
                    x: f64::from(r.loc.x),
                    y: f64::from(r.loc.y),
                    width: f64::from(r.size.w),
                    height: f64::from(r.size.h),
                });
            let mut local = point;
            let turn = self.layout_pointer_turn(&target);
            if let Some((_, _, presented)) = &turn {
                local = inverse(point, presented.pivot, presented.angle);
                if let Some((_, captured)) = presented
                    .members
                    .iter()
                    .find(|(id, _)| *id == self.windows[index].id)
                {
                    rect = *captured;
                }
            }
            if let Some((_, owner, presented)) = &turn {
                if *owner != target && !contains(presented.bounds, local) {
                    continue;
                }
            } else if self.layout_group_clip(index).is_some_and(|clip| {
                !clip.contains((local.x.round() as i32, local.y.round() as i32))
            }) {
                continue;
            }
            if contains(rect, local) {
                return Some(target);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inverse_turn_finds_the_visible_grab_outside_the_final_upright_bounds() {
        let bounds = Rect {
            x: 100.,
            y: 120.,
            width: 54.,
            height: 200.,
        };
        let pivot = Point { x: 82., y: 170. };
        let local = Point { x: 127., y: 290. };
        let visible = inverse(local, pivot, std::f64::consts::FRAC_PI_2);
        assert!(!contains(bounds, visible));
        let grabbed = inverse(visible, pivot, -std::f64::consts::FRAC_PI_2);
        assert!((grabbed.x - local.x).abs() < 1e-8 && (grabbed.y - local.y).abs() < 1e-8);
        let upright = Rect {
            x: visible.x - (grabbed.x - bounds.x),
            y: visible.y - (grabbed.y - bounds.y),
            ..bounds
        };
        assert!(contains(upright, visible));
        assert!((visible.x - upright.x - 27.).abs() < 1e-8);
        assert!((visible.y - upright.y - 170.).abs() < 1e-8);
    }
}
