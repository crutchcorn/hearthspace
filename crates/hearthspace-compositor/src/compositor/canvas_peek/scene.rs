//! Adapt native surface geometry and stable layout identities to canvas rectangles.
use super::{App, Point, Rect, rect};
use hearthspace_common::{
    canvas_peek::{Decoration, Scene, Window, icon_body},
    frame_layout::{ObjectPlacement, TargetKind},
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Snapshot {
    pub bounds: Rect,
    pub background: bool,
    decoration: bool,
}

impl App {
    pub(super) fn canvas_peek_scene(&self) -> (Scene, BTreeMap<u64, Snapshot>) {
        let mut scene = Scene {
            windows: Vec::new(),
            decorations: Vec::new(),
            obstacles: Vec::new(),
        };
        let mut snapshot = BTreeMap::new();
        for (index, window) in self.windows.iter().enumerate() {
            if !window.mapped {
                continue;
            }
            let target = self.frame_target_for_window(index);
            let placed = target.as_ref().and_then(|target| {
                self.layout_editor
                    .placed
                    .iter()
                    .find(|p| &p.target == target)
            });
            if placed.is_some_and(|p| {
                p.placement.is_frame()
                    || p.placement == ObjectPlacement::Hidden
                    || p.group.is_some()
            }) {
                continue;
            }
            if self.frame_window_attached(index) {
                continue;
            }
            let background = placed.map_or(self.desktop_id(index).is_some(), |p| {
                matches!(p.placement, ObjectPlacement::Background { .. })
            });
            let is_icon = self.desktop_id(index).is_some();
            let is_control = self.frame_widget_identity(index).is_some();
            if !is_control && !window.initial_placement_done {
                continue;
            }
            if !self.is_navigable_window(index) && !is_icon && !is_control {
                continue;
            }
            if is_control && placed.is_none() {
                continue;
            }
            let mut bounds = if is_control {
                let p = placed.unwrap();
                // Placements are measured in output coordinates, independently
                // of native surface padding and any animated display pose.
                Rect {
                    x: p.rect.x / self.viewport_scale + f64::from(self.viewport_offset.x),
                    y: p.rect.y / self.viewport_scale + f64::from(self.viewport_offset.y),
                    width: p.rect.width / self.viewport_scale,
                    height: p.rect.height / self.viewport_scale,
                }
            } else {
                rect(self.window_canvas_rect(index))
            };
            // Projection followed by its inverse can differ by a few ulps
            // after zooming. Viewport motion must never trigger settlement.
            bounds.x = (bounds.x * 1_000_000.).round() / 1_000_000.;
            bounds.y = (bounds.y * 1_000_000.).round() / 1_000_000.;
            bounds.width = (bounds.width * 1_000_000.).round() / 1_000_000.;
            bounds.height = (bounds.height * 1_000_000.).round() / 1_000_000.;
            if bounds.width <= 0. || bounds.height <= 0. {
                continue;
            }
            let decoration = is_icon || is_control;
            snapshot.insert(
                window.id,
                Snapshot {
                    bounds,
                    background,
                    decoration,
                },
            );
            if decoration {
                let home = self
                    .canvas_peek_key(index)
                    .and_then(|key| self.canvas_peek.homes.get(&key).copied());
                scene.decorations.push(Decoration {
                    id: window.id,
                    bounds,
                    body: if is_icon { icon_body(bounds) } else { bounds },
                    home,
                    raised: !background,
                });
            } else {
                scene.windows.push(Window {
                    id: window.id,
                    bounds,
                    background,
                    fixed: false,
                });
            }
        }
        // A background window is below raised controls even when recently focused.
        scene.windows.sort_by_key(|window| !window.background);
        (scene, snapshot)
    }

    pub(in crate::compositor) fn canvas_peek_key(&self, index: usize) -> Option<String> {
        if let Some(id) = self.desktop_id(index) {
            return Some(format!("item:{id}"));
        }
        let identity = self.frame_widget_identity(index)?;
        Some(format!(
            "widget:{}",
            serde_json::to_string(&identity).unwrap()
        ))
    }

    pub(super) fn translate_canvas_peek_target(&mut self, index: usize, delta: Point) {
        if self.frame_widget_identity(index).is_some() {
            if let Some(target) = self.frame_target_for_window(index)
                && let Some(layout) = self.layout_editor.layouts.get_mut(&target.output)
            {
                let mut placement = layout.placement(&target.target);
                if let ObjectPlacement::Canvas { x, y, .. }
                | ObjectPlacement::Background { x, y, .. } = &mut placement
                {
                    *x = Some(x.unwrap_or(0.) + delta.x);
                    *y = Some(y.unwrap_or(0.) + delta.y);
                }
                layout.set_placement(&target.target, placement);
            }
        } else {
            let position = self.windows[index].position;
            self.set_group_window_position(
                index,
                crate::geometry::CanvasPoint {
                    x: (f64::from(position.x) + delta.x).round() as i32,
                    y: (f64::from(position.y) + delta.y).round() as i32,
                },
            );
        }
    }

    pub(in crate::compositor) fn forget_canvas_peek_home(&mut self, index: usize) {
        if let Some(key) = self.canvas_peek_key(index) {
            self.canvas_peek.homes.remove(&key);
        }
    }
}

// These accessors keep private gesture and placement internals in their owners.
impl App {
    pub(in crate::compositor) fn canvas_peek_drop_for_target(
        &mut self,
        target: &super::super::layout_editor::Target,
    ) {
        let id = self.windows.iter().enumerate().find_map(|(index, w)| {
            let matches = match target.target.kind {
                TargetKind::Window => target.target.id == w.id.to_string(),
                TargetKind::Item => self.desktop_id(index).as_deref() == Some(&target.target.id),
                TargetKind::Widget => self
                    .frame_widget_identity(index)
                    .is_some_and(|(output, id)| output == target.output && id == target.target.id),
                TargetKind::Group => false,
            };
            (w.mapped && matches).then_some(w.id)
        });
        if let Some(id) = id {
            self.canvas_peek_layout_drop(id);
        }
    }
}
