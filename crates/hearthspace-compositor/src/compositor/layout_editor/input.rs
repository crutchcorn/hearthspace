//! Editing captures object presses before clients; one release belongs to one gesture.
use super::{App, LayoutTarget, ObjectPlacement, Point, Rect, Target, TargetKind};
use hearthspace_common::frame_layout::{self, FrameLayout};
use smithay::backend::input::ButtonState;
use std::collections::BTreeMap;
mod drop;
mod grid;
mod motion;

#[derive(Clone)]
pub(super) struct Drag {
    target: Target,
    start: Point,
    rect: Rect,
    moved: bool,
    edge: Option<frame_layout::FrameEdge>,
    layouts: BTreeMap<String, FrameLayout>,
    positions: Vec<(u64, crate::geometry::CanvasPoint)>,
}
impl App {
    pub(in crate::compositor) fn reset_layout_pointer_captures(&mut self) {
        self.layout_editor.suppress_release = false;
        self.layout_editor.consumed_buttons.clear();
    }
    pub(in crate::compositor) fn cancel_layout_drag(&mut self) {
        if self.layout_editor.pressed_item.take().is_some() {
            self.layout_editor.suppress_release = true;
        }
        if let Some(drag) = self.layout_editor.drag.take() {
            self.layout_editor.layouts = drag.layouts;
            for (id, position) in drag.positions {
                if let Some(window) = self.windows.iter_mut().find(|w| w.id == id) {
                    window.position = position;
                }
            }
            self.layout_editor.suppress_release = true;
            self.clear_layout_motion();
            self.clear_layout_turn();
            self.layout_editor.placed = self.frame_placements();
            self.layout_changed();
        }
    }
    pub(in crate::compositor) fn layout_escape(&mut self) {
        if self.layout_confirmation_active() {
            self.cancel_layout_confirmation();
        } else if self.layout_editor.drag.is_some() {
            self.cancel_layout_drag();
            self.finish_layout_editor();
        } else {
            self.finish_layout_editor();
        }
    }
    pub(in crate::compositor) fn layout_pointer_button(
        &mut self,
        button: u32,
        state: ButtonState,
    ) -> bool {
        if state == ButtonState::Released && self.layout_editor.consumed_buttons.remove(&button) {
            return true;
        }
        if button == 0x110 && state == ButtonState::Released && self.layout_editor.suppress_release
        {
            self.layout_editor.suppress_release = false;
            return true;
        }
        if self.layout_confirmation_active() {
            if self
                .pointer
                .grab_start_data()
                .and_then(|start| start.focus)
                .and_then(|(surface, _)| self.window_index_for_surface(&surface))
                .is_some_and(|index| self.layout_confirmation_surface(index))
            {
                return false;
            }
            let consumed=!self.hit_test(self.pointer_location).is_some_and(|hit| matches!(hit,crate::compositor::HitTarget::Client{window_index,..} if self.layout_confirmation_surface(window_index)));
            if consumed && state == ButtonState::Pressed {
                self.layout_editor.consumed_buttons.insert(button);
            }
            return consumed;
        }
        if button != 0x110 {
            return false;
        }
        if !self.layout_editing() {
            return self.pinned_item_button(state);
        }
        if state == ButtonState::Released {
            if let Some(drag) = self.layout_editor.drag.take() {
                if drag.moved {
                    let vertical = drag.layouts.get(&drag.target.output).is_some_and(|l| {
                        l.placement(&drag.target.target)
                            .edge()
                            .is_some_and(|e| e.is_vertical())
                    });
                    self.layout_editor.drop_anchor = Some((drag.start, drag.rect, vertical));
                    self.commit_layout_drop(&drag);
                    self.layout_editor.drop_anchor = None;
                }
                self.reconcile_layout_editor();
                self.layout_changed();
                return true;
            }
            return false;
        }
        if self.pointer.is_grabbed() {
            return false;
        }
        let point = Point {
            x: self.pointer_location.x,
            y: self.pointer_location.y,
        };
        if self.layout_native_editor_at(point) {
            return false;
        }
        let Some(target) = self.layout_target_at(point) else {
            self.layout_editor.consumed_buttons.insert(button);
            self.layout_editor.session.as_mut().unwrap().selected = None;
            self.layout_changed();
            return true;
        };
        let rect = self
            .frame_placed(&target)
            .map(|p| {
                if target.target.kind == TargetKind::Window && p.placement.is_frame() {
                    self.pinned_reveal_rect(p)
                } else {
                    p.rect
                }
            })
            .or_else(|| {
                self.target_window_index(&target).map(|i| {
                    let r = self.window_screen_bounds(i);
                    Rect {
                        x: f64::from(r.loc.x),
                        y: f64::from(r.loc.y),
                        width: f64::from(r.size.w),
                        height: f64::from(r.size.h),
                    }
                })
            })
            .unwrap_or_default();
        let rect = self
            .target_window_index(&target)
            .and_then(|index| self.layout_motion_rect(index))
            .unwrap_or(rect);
        let affected: Vec<_> = self
            .layout_editor
            .placed
            .iter()
            .filter(|p| {
                p.target.output == target.output
                    && (p.target == target
                        || (target.target.kind == TargetKind::Group
                            && p.group.as_deref() == Some(target.target.id.as_str())))
            })
            .filter_map(|p| {
                self.target_window_index(&p.target)
                    .map(|i| self.windows[i].id)
            })
            .collect();
        for id in affected {
            self.queue_layout_motion(id, rect, rect, 0);
        }
        self.layout_editor.session.as_mut().unwrap().selected = Some(target.clone());
        self.layout_editor.drag = Some(Drag {
            edge: self.target_placement(&target).edge(),
            target,
            start: point,
            rect,
            moved: false,
            layouts: self.layout_editor.layouts.clone(),
            positions: self.windows.iter().map(|w| (w.id, w.position)).collect(),
        });
        self.layout_changed();
        true
    }
    fn layout_target_at(&self, point: Point) -> Option<Target> {
        for placed in self.layout_editor.placed.iter().rev() {
            if placed.target.target.kind == TargetKind::Group
                && !super::compact::backing(&placed.target.target.id)
                && contains(
                    super::geometry::grip_rect(placed.rect, placed.placement.edge()),
                    point,
                )
            {
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
                || self.layout_group_clip(index).is_some_and(|clip| {
                    !clip.contains((point.x.round() as i32, point.y.round() as i32))
                })
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
                let output = self.layout_editor.session.as_ref()?.output.clone();
                Some(Target {
                    output,
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
            let rect = self.frame_placed(&target).map_or(
                Rect {
                    x: f64::from(r.loc.x),
                    y: f64::from(r.loc.y),
                    width: f64::from(r.size.w),
                    height: f64::from(r.size.h),
                },
                |p| {
                    if p.target.target.kind == TargetKind::Window && p.placement.is_frame() {
                        self.pinned_reveal_rect(p)
                    } else {
                        p.rect
                    }
                },
            );
            if contains(rect, point) {
                return Some(target);
            }
        }
        None
    }
    fn layout_native_editor_at(&self, point: Point) -> bool {
        self.windows.iter().enumerate().any(|(i, w)| {
            w.mapped
                && super::window_app_id(&w.surface)
                    .is_some_and(|id| frame_layout::layout_editor_from_app_id(&id).is_some())
                && self.layout_surface_visible(i)
                && self
                    .layout_target_rect(i)
                    .is_some_and(|rect| contains(rect, point))
        })
    }
    fn update_frame_hover(&mut self) {
        let point = Point {
            x: self.pointer_location.x,
            y: self.pointer_location.y,
        };
        let hovered = (!self.dispatching_touch)
            .then(|| {
                self.layout_editor
                    .placed
                    .iter()
                    .filter(|p| {
                        p.target.target.kind == TargetKind::Window && p.placement.is_frame()
                    })
                    .find(|p| contains(self.pinned_reveal_rect(p), point))
                    .and_then(|p| p.target.target.id.parse().ok())
            })
            .flatten();
        if hovered != self.layout_editor.hovered_window {
            let before: Vec<_> = self
                .windows
                .iter()
                .enumerate()
                .filter_map(|(i, w)| self.layout_target_rect(i).map(|rect| (w.id, rect)))
                .collect();
            self.layout_editor.hovered_window = hovered;
            for (id, from) in before {
                if let Some(i) = self.windows.iter().position(|w| w.id == id)
                    && let Some(to) = self.layout_target_rect(i)
                {
                    self.queue_layout_motion(id, from, to, super::motion::REVEAL_DURATION_MS);
                }
            }
            self.layout_changed();
        }
    }
}
pub(super) fn contains(rect: Rect, point: Point) -> bool {
    point.x >= rect.x
        && point.x < rect.x + rect.width
        && point.y >= rect.y
        && point.y < rect.y + rect.height
}

impl App {
    pub(in crate::compositor) fn active_drag_zone(&self) -> Option<frame_layout::GuideZone> {
        let drag = self.layout_editor.drag.as_ref().filter(|d| d.moved)?;
        let view = self.frame_output_rect(&drag.target.output)?;
        let point = Point {
            x: self.pointer_location.x - view.x,
            y: self.pointer_location.y - view.y,
        };
        if let Some(corner) = frame_layout::frame_corner_at_pointer(point, view) {
            return Some(frame_layout::GuideZone::Corner(corner));
        }
        let rect = self.frame_placed(&drag.target)?.rect;
        frame_layout::frame_drop_target(
            Rect {
                x: rect.x - view.x,
                y: rect.y - view.y,
                ..rect
            },
            view,
            72.,
            Some(point),
        )
        .and_then(|p| p.edge())
        .map(frame_layout::GuideZone::Edge)
    }
    fn pinned_item_at_pointer(&self) -> Option<String> {
        match self.hit_test(self.pointer_location) {
            Some(crate::compositor::HitTarget::Client { window_index, .. }) => {
                let target = self.frame_target_for_window(window_index)?;
                (target.target.kind == TargetKind::Item
                    && self.target_placement(&target).is_frame())
                .then_some(target.target.id)
            }
            _ => None,
        }
    }
    fn pinned_item_button(&mut self, state: ButtonState) -> bool {
        if state == ButtonState::Released
            && let Some(id) = self.layout_editor.pressed_item.take()
        {
            if self.pinned_item_at_pointer().as_deref() == Some(&id) {
                let _ = self.desktop_action("activate", &id);
            }
            return true;
        }
        if state == ButtonState::Pressed
            && !self.pointer.is_grabbed()
            && let Some(id) = self.pinned_item_at_pointer()
        {
            self.layout_editor.pressed_item = Some(id);
            return true;
        }
        false
    }
}
