//! Guides sit below frame tools; selection outlines follow their owning objects.
use super::{App, Rect, Target, overlays::OverlayShape};
use hearthspace_common::{
    canvas_search::motion::{ease_out, opacity_ease},
    frame_layout::{GuideZone, LayoutTarget, TargetKind, guide_rects},
};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

const GUIDE_DURATION: Duration = Duration::from_millis(160);

pub(super) struct GuideMotion {
    advancing: bool,
    zone: Option<GuideZone>,
    changed: Instant,
    from: [f32; 8],
}

impl Default for GuideMotion {
    fn default() -> Self {
        Self {
            advancing: false,
            zone: None,
            changed: Instant::now() - GUIDE_DURATION,
            from: [0.; 8],
        }
    }
}

impl GuideMotion {
    fn weights(&self, view: Rect, reduced: bool) -> [f32; 8] {
        let progress = if reduced {
            1.0
        } else {
            opacity_ease(self.changed.elapsed().as_secs_f64() / GUIDE_DURATION.as_secs_f64()) as f32
        };
        let zones = guide_rects(view);
        std::array::from_fn(|index| {
            let to = if self.zone == Some(zones[index].0) {
                1.0
            } else {
                0.0
            };
            self.from[index] + (to - self.from[index]) * progress
        })
    }
}

impl App {
    fn layout_outline_bounds(&self, target: &Target, bounds: Rect) -> Option<Rect> {
        let Some(clip) = self
            .target_window_index(target)
            .and_then(|index| self.layout_group_clip(index))
        else {
            return Some(bounds);
        };
        let x = bounds.x.max(f64::from(clip.loc.x));
        let y = bounds.y.max(f64::from(clip.loc.y));
        let width = (bounds.x + bounds.width).min(f64::from(clip.loc.x + clip.size.w)) - x;
        let height = (bounds.y + bounds.height).min(f64::from(clip.loc.y + clip.size.h)) - y;
        (width > 0. && height > 0.).then_some(Rect {
            x,
            y,
            width,
            height,
        })
    }

    pub(in crate::compositor) fn layout_outline_owner(
        &self,
        key: &str,
    ) -> Option<(u64, TargetKind)> {
        let target = LayoutTarget::from_key(
            key.strip_prefix("object-")
                .or_else(|| key.strip_prefix("drop-"))?,
        )?;
        let kind = target.kind;
        if key.starts_with("drop-") && kind != TargetKind::Item {
            return None;
        }
        let output = self.layout_editor.session.as_ref()?.output.clone();
        self.target_window_index(&Target { output, target })
            .map(|index| (self.windows[index].id, kind))
    }

    pub(in crate::compositor) fn modal_confirmation_shape(
        &self,
        output: smithay::utils::Rectangle<i32, smithay::utils::Logical>,
    ) -> Option<OverlayShape> {
        let layout_confirmation = self.layout_editor.session.as_ref().is_some_and(|session| {
            session.confirmation.is_some()
                && self.output_logical_rect_for_name(Some(&session.output)) == Some(output)
        });
        if !self.power_confirmation_on_output(output) && !layout_confirmation {
            return None;
        }
        let mut ink = self.palette().text.components;
        ink[3] = 0.18;
        Some(OverlayShape {
            key: "confirmation-modal".into(),
            bounds: Rect {
                x: f64::from(output.loc.x),
                y: f64::from(output.loc.y),
                width: f64::from(output.size.w),
                height: f64::from(output.size.h),
            },
            ink,
            radius: 0.,
            stroke: 0.,
            fill: 1.,
            dashed: false,
            opacity: 1.,
        })
    }
    pub(super) fn layout_guides_animating(&self) -> bool {
        self.layout_editing()
            && (self.layout_editor.guides.changed.elapsed() < GUIDE_DURATION
                || self
                    .layout_editor
                    .session
                    .as_ref()
                    .is_some_and(|session| session.started.elapsed() < GUIDE_DURATION))
    }
    pub(super) fn layout_guides_need_animation_frame(&self) -> bool {
        self.layout_editing() && self.layout_editor.guides.advancing
    }
    pub(in crate::compositor) fn advance_layout_guides(&mut self) {
        if !self.layout_editing() {
            self.layout_editor.guides = Default::default();
            return;
        }
        let zone = self.active_drag_zone();
        let reduced = self.layout_reduced_motion();
        if zone != self.layout_editor.guides.zone {
            self.layout_editor.guides.from =
                self.layout_editor.guides.weights(Rect::default(), reduced);
            self.layout_editor.guides.zone = zone;
            self.layout_editor.guides.changed = Instant::now();
        }
        // Keep one final timer tick when a fade expires between dispatches.
        let advancing = !reduced && self.layout_guides_animating();
        let was_advancing = std::mem::replace(&mut self.layout_editor.guides.advancing, advancing);
        if was_advancing || advancing {
            self.request_redraw();
        }
    }

    pub(in crate::compositor) fn layout_overlay_shapes(
        &self,
    ) -> (Vec<OverlayShape>, Vec<OverlayShape>) {
        let (mut back, mut front) = (Vec::new(), Vec::new());
        let Some(session) = self
            .layout_editor
            .session
            .as_ref()
            .filter(|_| self.layout_editing())
        else {
            return (back, front);
        };
        let Some(view) = self.frame_output_rect(&session.output) else {
            return (back, front);
        };
        let palette = self.palette();
        let accent = palette.accent.components;
        let border = palette.accent_border.components;
        let soft = palette.accent_soft.components;
        let alpha = if self.layout_reduced_motion() {
            1.0
        } else {
            ease_out(session.started.elapsed().as_secs_f64() / GUIDE_DURATION.as_secs_f64()) as f32
        };
        let weights = self
            .layout_editor
            .guides
            .weights(view, self.layout_reduced_motion());
        for ((zone, mut bounds), weight) in guide_rects(view).into_iter().zip(weights) {
            bounds.x += view.x;
            bounds.y += view.y;
            let ink = std::array::from_fn(|i| border[i] + (accent[i] - border[i]) * weight);
            let base = [accent[0], accent[1], accent[2], 0.03];
            let fill_ink = std::array::from_fn(|i| base[i] + (soft[i] - base[i]) * weight);
            let opacity = alpha * (0.4 + 0.6 * weight);
            back.push(OverlayShape {
                key: format!("guide-fill-{zone:?}"),
                bounds,
                ink: fill_ink,
                radius: 20.,
                stroke: 0.,
                fill: 1.,
                dashed: false,
                opacity,
            });
            front.push(OverlayShape {
                key: format!("guide-border-{zone:?}"),
                bounds,
                ink,
                radius: 20.,
                stroke: 1.,
                fill: 0.,
                dashed: false,
                opacity,
            });
        }
        let drop_target = self.active_layout_drop_target();
        let mut seen = BTreeSet::new();
        for placed in &self.layout_editor.placed {
            if placed.target.output != session.output {
                continue;
            }
            if super::compact::backing(&placed.target.target.id) {
                continue;
            }
            seen.insert(placed.target.target.key());
            let dropping = drop_target.as_ref() == Some(&placed.target);
            let selected = session.selected.as_ref() == Some(&placed.target) || dropping;
            let mut bounds =
                if placed.target.target.kind == TargetKind::Window && placed.placement.is_frame() {
                    self.pinned_reveal_rect(placed)
                } else {
                    self.target_window_index(&placed.target)
                        .and_then(|index| self.layout_motion_rect(index))
                        .unwrap_or(placed.rect)
                };
            if let Some(index) = self.target_window_index(&placed.target) {
                let offset = self.canvas_peek_offset(index);
                bounds.x += offset.x;
                bounds.y += offset.y;
            }
            let Some(bounds) = self.layout_outline_bounds(&placed.target, bounds) else {
                continue;
            };
            let inset = if placed.group.is_some() {
                -1.0
            } else if matches!(
                placed.target.target.kind,
                TargetKind::Widget | TargetKind::Group
            ) {
                4.0
            } else {
                3.0
            };
            front.push(outline(
                &placed.target.target.key(),
                bounds,
                inset,
                selected,
                accent,
                border,
            ));
            if dropping {
                front.push(OverlayShape {
                    key: format!("drop-{}", placed.target.target.key()),
                    bounds,
                    ink: soft,
                    radius: if placed.group.is_some() { 15. } else { 20. },
                    stroke: 0.,
                    fill: 1.,
                    dashed: false,
                    opacity: 1.,
                });
            }
        }
        // Ordinary windows and unmodified desktop items have no saved frame entry.
        for (index, window) in self.windows.iter().enumerate() {
            if !window.mapped {
                continue;
            }
            let target = if let Some(id) = self.desktop_id(index) {
                LayoutTarget::new(TargetKind::Item, id)
            } else if self.is_navigable_window(index) {
                LayoutTarget::new(TargetKind::Window, window.id.to_string())
            } else {
                continue;
            };
            if !seen.insert(target.key()) {
                continue;
            }
            let target = Target {
                output: session.output.clone(),
                target,
            };
            let r = self.window_screen_bounds(index);
            let bounds = Rect {
                x: f64::from(r.loc.x),
                y: f64::from(r.loc.y),
                width: f64::from(r.size.w),
                height: f64::from(r.size.h),
            };
            if bounds.x >= view.x + view.width
                || bounds.y >= view.y + view.height
                || bounds.x + bounds.width <= view.x
                || bounds.y + bounds.height <= view.y
            {
                continue;
            }
            front.push(outline(
                &target.target.key(),
                bounds,
                3.,
                session.selected.as_ref() == Some(&target),
                accent,
                border,
            ));
        }
        (back, front)
    }
}

fn outline(
    key: &str,
    bounds: Rect,
    inset: f64,
    selected: bool,
    accent: [f32; 4],
    border: [f32; 4],
) -> OverlayShape {
    OverlayShape {
        key: format!("object-{key}"),
        bounds: Rect {
            x: bounds.x - inset,
            y: bounds.y - inset,
            width: bounds.width + inset * 2.,
            height: bounds.height + inset * 2.,
        },
        ink: if selected { accent } else { border },
        radius: (if inset == 4. { 20. } else { 15. } + inset) as f32,
        stroke: if selected { 2. } else { 1. },
        fill: 0.,
        dashed: !selected,
        opacity: 1.,
    }
}
