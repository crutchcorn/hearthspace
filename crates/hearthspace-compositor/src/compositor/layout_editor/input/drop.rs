//! Drop priority and frame grouping are independent of pointer capture.
use super::*;
impl App {
    pub(super) fn commit_layout_drop(&mut self, drag: &Drag) {
        if drag.target.target.kind == TargetKind::Item
            && self.desktop_over_apps_corner()
            && drag.target.target.id.starts_with("app:")
        {
            self.layout_editor.layouts = drag.layouts.clone();
            for (id, position) in &drag.positions {
                if let Some(window) = self.windows.iter_mut().find(|w| w.id == *id) {
                    window.position = *position;
                }
            }
            let _ = self.desktop_layout_favorite(&drag.target.target.id);
            return;
        }
        let point = Point {
            x: self.pointer_location.x,
            y: self.pointer_location.y,
        };
        let output = self
            .capture_outputs()
            .into_iter()
            .find(|o| {
                contains(
                    Rect {
                        x: f64::from(o.logical_x),
                        y: f64::from(o.logical_y),
                        width: f64::from(o.logical_width),
                        height: f64::from(o.logical_height),
                    },
                    point,
                )
            })
            .map(|o| o.name)
            .unwrap_or_else(|| drag.target.output.clone());
        if matches!(
            drag.target.target.kind,
            TargetKind::Widget | TargetKind::Group
        ) && output != drag.target.output
        {
            // Per-output controls keep their identity; another output may already own this widget.
            // Canvas coordinates can cross outputs without replacing that other control.
            if drag.target.target.kind == TargetKind::Group {
                self.release_canvas_group(&drag.target);
            } else {
                self.finish_canvas_widget_drop(drag);
            }
            return;
        }
        let Some(view) = self.frame_output_rect(&output) else {
            return;
        };
        let target = Target {
            output: output.clone(),
            target: drag.target.target.clone(),
        };
        let candidate = self.layout_drop_candidate(drag, &output, point);
        if let Some(destination) = candidate {
            if output != drag.target.output {
                self.layout_editor
                    .layouts
                    .get_mut(&drag.target.output)
                    .unwrap()
                    .remove_target(&drag.target.target);
            }
            let before = if destination.vertical {
                point.y < destination.rect.y + destination.rect.height / 2.
            } else {
                point.x < destination.rect.x + destination.rect.width / 2.
            };
            // Attaching changes presentation only; preserve the original canvas home.
            for (id, position) in &drag.positions {
                if let Some(window) = self.windows.iter_mut().find(|window| window.id == *id) {
                    window.position = *position;
                }
            }
            if let Some(group) = self.layout_editor.layouts.get_mut(&output).unwrap().merge(
                &target.target,
                &destination.target.target,
                before,
            ) {
                self.layout_editor.session.as_mut().unwrap().selected = Some(Target {
                    output,
                    target: group,
                });
                return;
            }
        }
        let current = self
            .frame_placed(&drag.target)
            .map_or(drag.rect, |p| p.rect);
        let local = Rect {
            x: current.x - view.x,
            y: current.y - view.y,
            ..current
        };
        let pointer = Some(Point {
            x: point.x - view.x,
            y: point.y - view.y,
        });
        let original = drag
            .layouts
            .get(&drag.target.output)
            .map(|l| l.placement(&drag.target.target));
        let placement = if matches!(target.target.kind, TargetKind::Widget | TargetKind::Group) {
            frame_layout::widget_frame_drop_target(
                local,
                view,
                72.,
                original.and_then(|p| p.edge()),
                pointer,
            )
        } else {
            frame_layout::frame_drop_target(local, view, 72., pointer)
        };
        if let Some(mut placement) = placement {
            if let Some(index) = self.target_window_index(&target)
                && let Some((_, position)) = drag
                    .positions
                    .iter()
                    .find(|(id, _)| *id == self.windows[index].id)
            {
                self.windows[index].position = *position;
            }
            if self.keyboard.modifier_state().alt {
                placement = frame_layout::snap_frame_placement(
                    placement,
                    local,
                    view,
                    super::grid::origin(self.viewport_offset, self.viewport_scale, view),
                    f64::from(crate::geometry::grid::CANVAS_GRID_SIZE) * self.viewport_scale,
                    target.target.kind != TargetKind::Window,
                );
            }
            self.place_layout_target(&target, placement);
            self.layout_editor.session.as_mut().unwrap().selected = Some(target);
        } else if target.target.kind == TargetKind::Group {
            self.release_canvas_group(&target);
            self.layout_editor.session.as_mut().unwrap().selected = None;
        } else if target.target.kind == TargetKind::Widget {
            self.finish_canvas_widget_drop(drag);
        }
    }
}

impl App {
    fn layout_drop_candidate(
        &self,
        drag: &Drag,
        output: &str,
        point: Point,
    ) -> Option<super::super::PlacedTarget> {
        self.layout_editor
            .placed
            .iter()
            .filter(|p| {
                p.target.output == output
                    && p.target != drag.target
                    && p.placement.is_frame()
                    && !super::super::compact::backing(&p.target.target.id)
            })
            .filter_map(|p| {
                let mut candidate = p.clone();
                if candidate.target.target.kind == TargetKind::Window {
                    candidate.rect = self.pinned_reveal_rect(p);
                }
                let clip = p
                    .group
                    .as_ref()
                    .and_then(|id| {
                        self.frame_placed(&Target {
                            output: p.target.output.clone(),
                            target: LayoutTarget::new(TargetKind::Group, id),
                        })
                    })
                    .map(|group| self.frame_group_clip_rect(group));
                if let Some(clip) = clip {
                    let right = (candidate.rect.x + candidate.rect.width).min(clip.x + clip.width);
                    let bottom =
                        (candidate.rect.y + candidate.rect.height).min(clip.y + clip.height);
                    candidate.rect.x = candidate.rect.x.max(clip.x);
                    candidate.rect.y = candidate.rect.y.max(clip.y);
                    candidate.rect.width = right - candidate.rect.x;
                    candidate.rect.height = bottom - candidate.rect.y;
                    if candidate.rect.width <= 0.
                        || candidate.rect.height <= 0.
                        || !contains(clip, point)
                    {
                        return None;
                    }
                }
                contains(
                    Rect {
                        x: candidate.rect.x - 8.,
                        y: candidate.rect.y - 8.,
                        width: candidate.rect.width + 16.,
                        height: candidate.rect.height + 16.,
                    },
                    point,
                )
                .then_some(candidate)
            })
            .min_by(|a, b| {
                (a.rect.width * a.rect.height).total_cmp(&(b.rect.width * b.rect.height))
            })
    }
    pub(in crate::compositor) fn active_layout_drop_target(&self) -> Option<Target> {
        let drag = self.layout_editor.drag.as_ref().filter(|drag| drag.moved)?;
        if drag.target.target.kind == TargetKind::Item
            && self.desktop_over_apps_corner()
            && drag.target.target.id.starts_with("app:")
        {
            return None;
        }
        let point = Point {
            x: self.pointer_location.x,
            y: self.pointer_location.y,
        };
        let output = self.layout_editor.layouts.keys().find(|output| {
            self.frame_output_rect(output)
                .is_some_and(|view| contains(view, point))
        })?;
        if matches!(
            drag.target.target.kind,
            TargetKind::Widget | TargetKind::Group
        ) && *output != drag.target.output
        {
            return None;
        }
        self.layout_drop_candidate(drag, output, point)
            .map(|p| p.target)
    }
    fn finish_canvas_widget_drop(&mut self, drag: &Drag) {
        let original_vertical = super::super::turns_as_toolbar(&drag.target.target)
            && drag.layouts.get(&drag.target.output).is_some_and(|l| {
                l.placement(&drag.target.target)
                    .edge()
                    .is_some_and(|e| e.is_vertical())
            });
        let (width, height) = if original_vertical {
            (drag.rect.height, drag.rect.width)
        } else {
            (drag.rect.width, drag.rect.height)
        };
        let x = ((drag.start.x - drag.rect.x) / drag.rect.width.max(1.)).clamp(0., 1.);
        let y = ((drag.start.y - drag.rect.y) / drag.rect.height.max(1.)).clamp(0., 1.);
        let (along, across) = if original_vertical { (y, x) } else { (x, y) };
        let canvas = crate::geometry::screen_to_canvas(
            (
                self.pointer_location.x - width * along,
                self.pointer_location.y - height * across,
            )
                .into(),
            self.viewport_offset,
            self.viewport_scale,
        );
        let placement = if drag.layouts.get(&drag.target.output).is_some_and(|l| {
            matches!(
                l.placement(&drag.target.target),
                ObjectPlacement::Background { .. }
            )
        }) {
            ObjectPlacement::Background {
                x: Some(canvas.x),
                y: Some(canvas.y),
                width: None,
                height: None,
            }
        } else {
            ObjectPlacement::Canvas {
                x: Some(canvas.x),
                y: Some(canvas.y),
                width: None,
                height: None,
            }
        };
        self.place_layout_target(&drag.target, placement);
    }
}
