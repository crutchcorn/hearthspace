//! One projection supplies rendering, input, and native surface measurements.
use super::{App, LayoutTarget, ObjectPlacement, PlacedTarget, Rect, Target, TargetKind};
use hearthspace_common::frame_layout::{self, FrameLayout, WIDGETS};
mod groups;
mod pinned;

pub(super) fn grip_rect(rect: Rect, edge: Option<frame_layout::FrameEdge>) -> Rect {
    use frame_layout::FrameEdge;
    match edge {
        Some(FrameEdge::Top) => Rect {
            x: rect.x + (rect.width - 28.) / 2.,
            y: rect.y + rect.height + 6.,
            width: 28.,
            height: 24.,
        },
        Some(FrameEdge::Left) => Rect {
            x: rect.x + rect.width + 6.,
            y: rect.y + (rect.height - 28.) / 2.,
            width: 24.,
            height: 28.,
        },
        Some(FrameEdge::Right) => Rect {
            x: rect.x - 30.,
            y: rect.y + (rect.height - 28.) / 2.,
            width: 24.,
            height: 28.,
        },
        _ => Rect {
            x: rect.x + (rect.width - 28.) / 2.,
            y: rect.y - 30.,
            width: 28.,
            height: 24.,
        },
    }
}
pub(super) fn json_rect(rect: Rect) -> serde_json::Value {
    serde_json::json!({"x":rect.x,"y":rect.y,"width":rect.width,"height":rect.height})
}
impl App {
    pub(super) fn layout_grip_rect(&self, placed: &PlacedTarget) -> Rect {
        let scale = if placed.placement.is_frame() {
            1.
        } else {
            self.viewport_scale
        };
        let grip = grip_rect(
            Rect {
                x: placed.rect.x / scale,
                y: placed.rect.y / scale,
                width: placed.rect.width / scale,
                height: placed.rect.height / scale,
            },
            self.layout_drag_edge(&placed.target)
                .or(placed.placement.edge()),
        );
        Rect {
            x: grip.x * scale,
            y: grip.y * scale,
            width: grip.width * scale,
            height: grip.height * scale,
        }
    }

    pub(in crate::compositor) fn frame_output_rect(&self, name: &str) -> Option<Rect> {
        let r = self.output_logical_rect_for_name(Some(name))?;
        Some(Rect {
            x: f64::from(r.loc.x),
            y: f64::from(r.loc.y),
            width: f64::from(r.size.w),
            height: f64::from(r.size.h),
        })
    }
    fn frame_target_size(
        &self,
        target: &Target,
        grouped: bool,
        vertical: bool,
        view: Rect,
    ) -> (f64, f64) {
        let native = self.target_window_index(target).map(|i| {
            let size = if target.target.kind == TargetKind::Window {
                self.window_canvas_rect(i).size
            } else {
                self.window_geometry_size(i)
            };
            (f64::from(size.w.max(1)), f64::from(size.h.max(1)))
        });
        match target.target.kind {
            TargetKind::Window if grouped => {
                let (w, h) = native.unwrap_or((640., 480.));
                if vertical {
                    (44., h.min((view.height - 48.).max(44.)))
                } else {
                    (w.min((view.width - 48.).max(44.)), 44.)
                }
            }
            TargetKind::Item => {
                if grouped {
                    (44., 44.)
                } else if self.target_placement(target).is_frame() {
                    (54., 54.)
                } else {
                    (
                        f64::from(hearthspace_common::DESKTOP_ITEM_WIDTH),
                        f64::from(hearthspace_common::DESKTOP_ITEM_HEIGHT),
                    )
                }
            }
            TargetKind::Widget => native
                .map(|(width, height)| {
                    if super::turns_as_toolbar(&target.target) {
                        let short = width.min(height);
                        let long = width.max(height);
                        if vertical {
                            (short, long)
                        } else {
                            (long, short)
                        }
                    } else {
                        (width, height)
                    }
                })
                .unwrap_or_else(|| {
                    let long = match target.target.id.as_str() {
                        "notifications" => 132.,
                        "clock" => 104.,
                        "tools" => 194.,
                        "apps" => 54.,
                        _ => 44.,
                    };
                    if vertical && super::turns_as_toolbar(&target.target) {
                        (if grouped { 44. } else { 54. }, long)
                    } else {
                        (long, if grouped { 44. } else { 54. })
                    }
                }),
            _ => native.unwrap_or((640., 480.)),
        }
    }
    fn standalone_frame_rect(
        &self,
        target: &Target,
        placement: ObjectPlacement,
        size: (f64, f64),
        view: Rect,
    ) -> Rect {
        if placement.is_frame() {
            let mut rect = frame_layout::frame_bounds(
                placement,
                size.0,
                size.1,
                view,
                target.target.kind != TargetKind::Window,
            );
            rect.x += view.x;
            rect.y += view.y;
            return rect;
        }
        let (mut x, mut y, _, _) = placement.canvas_bounds().unwrap_or_default();
        if matches!(target.target.kind, TargetKind::Window | TargetKind::Item) {
            x = None;
            y = None;
        }
        let canonical = self
            .target_window_index(target)
            .map(|i| self.window_canvas_rect(i));
        let x = x.unwrap_or_else(|| {
            canonical.map_or(f64::from(self.viewport_offset.x) + 120., |r| {
                f64::from(r.loc.x)
            })
        });
        let y = y.unwrap_or_else(|| {
            canonical.map_or(f64::from(self.viewport_offset.y) + 120., |r| {
                f64::from(r.loc.y)
            })
        });
        let width = size.0 * self.viewport_scale;
        let height = size.1 * self.viewport_scale;
        let point = self
            .layout_drag_origin(target, width, height)
            .map(|point| (point.x, point.y).into())
            .unwrap_or_else(|| self.canvas_to_screen((x, y).into()));
        Rect {
            x: point.x,
            y: point.y,
            width,
            height,
        }
    }
    pub(super) fn layout_target_size(
        &self,
        target: &Target,
        vertical: bool,
        view: Rect,
    ) -> (f64, f64) {
        if target.target.kind == TargetKind::Group {
            self.layout_editor
                .layouts
                .get(&target.output)
                .and_then(|layout| self.frame_group_measurement(layout, target, view, vertical))
                .map(|measurement| measurement.size)
                .unwrap_or((54., 54.))
        } else {
            self.frame_target_size(target, false, vertical, view)
        }
    }
    pub(super) fn frame_placements(&self) -> Vec<PlacedTarget> {
        let mut result = Vec::new();
        for (output, layout) in &self.layout_editor.layouts {
            let Some(view) = self.frame_output_rect(output) else {
                continue;
            };
            let mut targets: Vec<_> = WIDGETS.iter().map(|id| LayoutTarget::widget(*id)).collect();
            for key in layout.placements.keys() {
                if let Some(target) = LayoutTarget::from_key(key)
                    && !targets.contains(&target)
                {
                    targets.push(target);
                }
            }
            for group in layout.groups.values() {
                let target = LayoutTarget::new(TargetKind::Group, &group.id);
                if !targets.contains(&target) {
                    targets.push(target);
                }
            }
            for target in targets {
                if target.kind == TargetKind::Widget
                    && !self.frame_widget_available(output, &target.id)
                {
                    continue;
                }
                if target.kind == TargetKind::Window
                    && self
                        .target_window_index(&Target {
                            output: output.clone(),
                            target: target.clone(),
                        })
                        .is_none()
                {
                    continue;
                }
                if target.kind != TargetKind::Group && layout.group_for(&target).is_some() {
                    continue;
                }
                let placement = layout.placement(&target);
                if placement == ObjectPlacement::Hidden {
                    continue;
                }
                let vertical = self
                    .layout_drag_vertical(&Target {
                        output: output.clone(),
                        target: target.clone(),
                    })
                    .unwrap_or_else(|| placement.edge().is_some_and(|e| e.is_vertical()));
                let target = Target {
                    output: output.clone(),
                    target,
                };
                if target.target.kind == TargetKind::Group {
                    self.place_frame_group(layout, &target, placement, view, vertical, &mut result);
                } else {
                    let size = self.frame_target_size(&target, false, vertical, view);
                    let rect = self.standalone_frame_rect(&target, placement, size, view);
                    result.push(PlacedTarget {
                        target,
                        rect,
                        placement,
                        group: None,
                        vertical,
                    });
                }
            }
        }
        self.project_compact_frame(&mut result);
        self.project_frame_scroll(&mut result);
        result
    }
}
