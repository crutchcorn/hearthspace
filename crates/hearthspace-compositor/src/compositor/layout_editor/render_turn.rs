//! Compose each toolbar as one object before its orientation transition.
use super::{App, Target, turns_as_toolbar};
use crate::compositor::rendering::{HearthspaceRenderElement, TranslatedHearthspaceRenderElement};
use hearthspace_common::frame_layout::{Point, Rect};
use smithay::{
    backend::renderer::{
        element::utils::{Relocate, RelocateRenderElement},
        gles::GlesRenderer,
    },
    utils::{Logical, Rectangle},
};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

impl App {
    fn move_pending_layout_turn(&mut self, renderer: &GlesRenderer, key: &str, scale: i32) {
        let Some(turn) = self
            .layout_editor
            .turn
            .turns
            .get(key)
            .filter(|turn| turn.started.is_none() && turn.pointer_anchor)
        else {
            return;
        };
        let Some(presented) = self.layout_editor.turn.presented.get(key) else {
            return;
        };
        let Some(placed) = self.frame_placed(&turn.target) else {
            return;
        };
        let grip = super::grip::ToolbarGrip::at(turn.bounds, turn.pivot, placed.vertical);
        let local = grip.pivot(
            presented.bounds.width,
            presented.bounds.height,
            presented.vertical,
        );
        let (sin, cos) = presented.angle.sin_cos();
        let x = presented.bounds.x + local.x - presented.pivot.x;
        let y = presented.bounds.y + local.y - presented.pivot.y;
        let anchor = Point {
            x: presented.pivot.x + cos * x - sin * y,
            y: presented.pivot.y + sin * x + cos * y,
        };
        let desired = if self.layout_drag_vertical(&turn.target).is_some() {
            Point {
                x: self.pointer_location.x,
                y: self.pointer_location.y,
            }
        } else {
            Point {
                x: turn.pivot.x + turn.translation.x,
                y: turn.pivot.y + turn.translation.y,
            }
        };
        let physical = Point {
            x: ((desired.x - anchor.x) * f64::from(scale)).round(),
            y: ((desired.y - anchor.y) * f64::from(scale)).round(),
        };
        let delta = Point {
            x: physical.x / f64::from(scale),
            y: physical.y / f64::from(scale),
        };
        self.layout_editor
            .turn
            .cache
            .move_held(renderer, key, scale, physical);
        let presented = self.layout_editor.turn.presented.get_mut(key).unwrap();
        presented.bounds.x += delta.x;
        presented.bounds.y += delta.y;
        presented.pivot.x += delta.x;
        presented.pivot.y += delta.y;
        for (_, rect) in &mut presented.members {
            rect.x += delta.x;
            rect.y += delta.y;
        }
    }

    /// Compose after backdrop sampling so glass, shadows, and outlines rotate together.
    pub(in crate::compositor) fn apply_layout_turns(
        &mut self,
        renderer: &mut GlesRenderer,
        elements: &mut Vec<TranslatedHearthspaceRenderElement>,
        spans: &[(u64, Range<usize>)],
        output: Rectangle<i32, Logical>,
    ) {
        let mut targets: Vec<Target> = self
            .layout_editor
            .turn
            .turns
            .values()
            .map(|turn| turn.target.clone())
            .collect();
        if self.layout_editing() && !self.layout_reduced_motion() {
            for placed in &self.layout_editor.placed {
                if placed.group.is_none()
                    && turns_as_toolbar(&placed.target.target)
                    && !targets.contains(&placed.target)
                {
                    targets.push(placed.target.clone());
                }
            }
        }
        let keys: Vec<_> = targets
            .iter()
            .map(|target| format!("{}/{}", target.output, target.target.key()))
            .collect();
        self.layout_editor.turn.cache.retain(keys.iter());
        self.layout_editor
            .turn
            .presented
            .retain(|key, _| keys.contains(key));
        let scale = self
            .capture_outputs()
            .iter()
            .find(|candidate| {
                candidate.logical_x == output.loc.x
                    && candidate.logical_y == output.loc.y
                    && candidate.logical_width == output.size.w
                    && candidate.logical_height == output.size.h
            })
            .map_or(1, |candidate| candidate.scale);
        let mut replaced = HashSet::new();
        let mut replacements = HashMap::new();
        let now = self.layout_editor.motion.sampled_at;
        for target in targets {
            if self.output_logical_rect_for_name(Some(&target.output)) != Some(output) {
                continue;
            }
            let key = format!("{}/{}", target.output, target.target.key());
            self.move_pending_layout_turn(renderer, &key, scale);
            let Some(placed) = self.frame_placed(&target) else {
                continue;
            };
            let turn = self.layout_editor.turn.turns.get(&key);
            let active = turn.is_some();
            let pending = turn.is_some_and(|turn| turn.started.is_none());
            let bounds = self
                .target_window_index(&target)
                .and_then(|index| self.layout_motion_rect(index))
                .unwrap_or(placed.rect);
            let (pivot, angle) = turn.map_or(
                (
                    Point {
                        x: bounds.x + bounds.width / 2.,
                        y: bounds.y + bounds.height / 2.,
                    },
                    0.,
                ),
                |turn| {
                    (
                        Point {
                            x: bounds.x + turn.pivot.x - turn.bounds.x,
                            y: bounds.y + turn.pivot.y - turn.bounds.y,
                        },
                        turn.angle(now),
                    )
                },
            );
            let members = self.layout_turn_members(&target);
            let mut indices: Vec<_> = spans
                .iter()
                .filter(|(id, _)| members.contains(id))
                .flat_map(|(_, range)| range.clone())
                .filter(|index| !replaced.contains(index))
                .collect();
            indices.sort_unstable();
            indices.dedup();
            let Some(first) = indices.first().copied() else {
                continue;
            };
            let member_bounds = members
                .iter()
                .filter_map(|id| {
                    let index = self.windows.iter().position(|window| window.id == *id)?;
                    let rect = self
                        .layout_motion_rect(index)
                        .or_else(|| self.layout_target_rect(index))?;
                    Some((*id, rect))
                })
                .collect();
            let presented = super::turn::Presented {
                bounds,
                pivot,
                angle,
                members: member_bounds,
                vertical: placed.vertical,
            };
            let bounds = Rect {
                x: (bounds.x - f64::from(output.loc.x)) * f64::from(scale),
                y: (bounds.y - f64::from(output.loc.y)) * f64::from(scale),
                width: bounds.width * f64::from(scale),
                height: bounds.height * f64::from(scale),
            };
            let pivot = Point {
                x: (pivot.x - f64::from(output.loc.x)) * f64::from(scale),
                y: (pivot.y - f64::from(output.loc.y)) * f64::from(scale),
            };
            let cache = &mut self.layout_editor.turn.cache;
            let result = if pending {
                Ok(cache.held(renderer, &key, scale))
            } else {
                let scene: Vec<_> = indices.iter().map(|index| &elements[*index]).collect();
                cache.element(renderer, &key, &scene, bounds, pivot, angle, scale)
            };
            if !pending && matches!(result, Ok(Some(_))) {
                self.layout_editor.turn.presented.insert(key, presented);
            }
            match result {
                Ok(Some(element)) if active => {
                    replacements.insert(
                        first,
                        TranslatedHearthspaceRenderElement::from(
                            RelocateRenderElement::from_element(
                                HearthspaceRenderElement::from(element),
                                (0, 0),
                                Relocate::Relative,
                            ),
                        ),
                    );
                    replaced.extend(indices);
                }
                Err(error) => tracing::warn!(%error, "toolbar turn capture failed"),
                _ => {}
            }
        }
        let mut result = Vec::with_capacity(elements.len());
        for (index, element) in std::mem::take(elements).into_iter().enumerate() {
            if let Some(replacement) = replacements.remove(&index) {
                result.push(replacement);
            }
            if !replaced.contains(&index) {
                result.push(element);
            }
        }
        *elements = result;
    }
}

#[cfg(all(test, feature = "e2e"))]
#[path = "render_turn_tests.rs"]
mod tests;
