//! Drag orientation follows the frame preview while keeping the grabbed point anchored.
use super::*;
impl App {
    pub(in crate::compositor) fn layout_pointer_motion(&mut self) -> bool {
        if !self.layout_editing() {
            self.update_frame_hover();
            return false;
        }
        let Some(mut drag) = self.layout_editor.drag.take() else {
            return false;
        };
        let delta = Point {
            x: self.pointer_location.x - drag.start.x,
            y: self.pointer_location.y - drag.start.y,
        };
        if !drag.moved && delta.x.hypot(delta.y) < 6. {
            self.layout_editor.drag = Some(drag);
            return true;
        }
        drag.moved = true;
        let previous_vertical = drag.edge.is_some_and(|edge| edge.is_vertical());
        let screen = self.layout_drag_screen_origin(&mut drag, delta);
        let turning = previous_vertical != drag.edge.is_some_and(|edge| edge.is_vertical());
        let canvas =
            crate::geometry::screen_to_canvas(screen, self.viewport_offset, self.viewport_scale);
        let edge = self
            .frame_output_rect(&drag.target.output)
            .and_then(|view| {
                frame_layout::frame_drop_target(
                    Rect {
                        x: screen.x - view.x,
                        y: screen.y - view.y,
                        ..drag.rect
                    },
                    view,
                    72.,
                    Some(Point {
                        x: self.pointer_location.x - view.x,
                        y: self.pointer_location.y - view.y,
                    }),
                )
                .and_then(|placement| placement.edge())
            });
        let (x, y) =
            super::grid::align(canvas.x, canvas.y, self.keyboard.modifier_state().alt, edge);
        if let Some(index) = self.target_window_index(&drag.target)
            && matches!(
                drag.target.target.kind,
                TargetKind::Window | TargetKind::Item
            )
        {
            let bounds = self.window_canvas_rect(index);
            let position = self.windows[index].position;
            self.windows[index].pending_resize = None;
            self.windows[index].position = crate::geometry::CanvasPoint {
                x: x.round() as i32 - (bounds.loc.x - position.x),
                y: y.round() as i32 - (bounds.loc.y - position.y),
            };
        }
        let placement = if drag.layouts.get(&drag.target.output).is_some_and(|layout| {
            matches!(
                layout.placement(&drag.target.target),
                ObjectPlacement::Background { .. }
            )
        }) {
            ObjectPlacement::Background {
                x: Some(x),
                y: Some(y),
                width: None,
                height: None,
            }
        } else {
            ObjectPlacement::Canvas {
                x: Some(x),
                y: Some(y),
                width: None,
                height: None,
            }
        };
        self.layout_editor
            .layouts
            .get_mut(&drag.target.output)
            .unwrap()
            .set_placement(&drag.target.target, placement);
        self.layout_editor.drag = Some(drag.clone());
        self.layout_editor.placed = self.frame_placements();
        if turning && let Some(placed) = self.frame_placed(&drag.target).cloned() {
            let members = self
                .layout_editor
                .placed
                .iter()
                .filter(|p| {
                    p.target.output == drag.target.output
                        && (p.target == drag.target
                            || p.group.as_deref() == Some(drag.target.target.id.as_str()))
                })
                .filter_map(|p| {
                    self.target_window_index(&p.target)
                        .map(|i| self.windows[i].id)
                })
                .collect();
            self.queue_layout_turn(
                drag.target.clone(),
                members,
                placed.rect,
                Point {
                    x: self.pointer_location.x,
                    y: self.pointer_location.y,
                },
                if previous_vertical {
                    std::f64::consts::FRAC_PI_2
                } else {
                    -std::f64::consts::FRAC_PI_2
                },
            );
            self.layout_changed();
        }
        self.request_redraw();
        true
    }
}

impl App {
    fn layout_drag_screen_origin(
        &self,
        drag: &mut Drag,
        delta: Point,
    ) -> smithay::utils::Point<f64, smithay::utils::Logical> {
        if !super::super::turns_as_toolbar(&drag.target.target) {
            return (drag.rect.x + delta.x, drag.rect.y + delta.y).into();
        }
        let original_vertical = drag.layouts.get(&drag.target.output).is_some_and(|layout| {
            layout
                .placement(&drag.target.target)
                .edge()
                .is_some_and(|edge| edge.is_vertical())
        });
        if let Some(view) = self.frame_output_rect(&drag.target.output) {
            let point = Point {
                x: self.pointer_location.x - view.x,
                y: self.pointer_location.y - view.y,
            };
            if point.x >= 0. && point.x < view.width && point.y >= 0. && point.y < view.height {
                let rect = Rect {
                    x: drag.rect.x + delta.x - view.x,
                    y: drag.rect.y + delta.y - view.y,
                    ..drag.rect
                };
                if let Some(placement) =
                    frame_layout::widget_frame_drop_target(rect, view, 72., drag.edge, Some(point))
                {
                    drag.edge = placement.edge();
                }
            }
        }
        let vertical = drag.edge.is_some_and(|edge| edge.is_vertical());
        let (width, height) = if original_vertical != vertical {
            (drag.rect.height, drag.rect.width)
        } else {
            (drag.rect.width, drag.rect.height)
        };
        let x = ((drag.start.x - drag.rect.x) / drag.rect.width.max(1.)).clamp(0., 1.);
        let y = ((drag.start.y - drag.rect.y) / drag.rect.height.max(1.)).clamp(0., 1.);
        let (along, across) = if original_vertical { (y, x) } else { (x, y) };
        (
            self.pointer_location.x - width * if vertical { across } else { along },
            self.pointer_location.y - height * if vertical { along } else { across },
        )
            .into()
    }
    pub(in crate::compositor::layout_editor) fn layout_drag_vertical(
        &self,
        target: &Target,
    ) -> Option<bool> {
        let drag = self.layout_editor.drag.as_ref().filter(|drag| drag.moved)?;
        (drag.target == *target
            || self
                .layout_editor
                .layouts
                .get(&target.output)
                .and_then(|l| l.group_for(&target.target))
                .is_some_and(|g| {
                    drag.target.output == target.output
                        && drag.target.target.kind == TargetKind::Group
                        && g.id == drag.target.target.id
                }))
        .then_some(drag.edge.is_some_and(|edge| edge.is_vertical()))
    }
    pub(in crate::compositor) fn layout_drag_contains(&self, index: usize) -> bool {
        self.frame_target_for_window(index)
            .is_some_and(|target| self.layout_drag_vertical(&target).is_some())
    }
}
