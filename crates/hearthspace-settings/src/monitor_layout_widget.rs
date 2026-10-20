use hearthspace_ipc::MonitorConfig;
use hearthspace_ui_components as components;
use xilem::masonry::accesskit::{Node, Role};
use xilem::masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef,
    QueryCtx, RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
};
use xilem::masonry::imaging::Painter;
use xilem::masonry::kurbo::{Axis, Point, Size, Vec2};
use xilem::masonry::layout::{LenReq, Length};
use xilem::masonry::{core::MutateCtx, layout::AsUnit};

use crate::{
    actions::MonitorEditorAction,
    monitors::{
        EMPTY_MONITOR_CANVAS_HEIGHT, EMPTY_MONITOR_CANVAS_WIDTH, monitor_logical_height,
        monitor_logical_width,
    },
};

mod geometry;
mod keyboard;
mod paint;
use geometry::*;

const DRAG_START_DISTANCE: f64 = 3.0;

#[derive(Debug)]
pub(crate) struct MonitorLayoutWidget {
    monitors: Vec<MonitorConfig>,
    selected_monitor_id: Option<String>,
    drag: Option<MonitorDrag>,
}

#[derive(Debug, Clone)]
struct MonitorDrag {
    id: String,
    start_pointer: Point,
    grab_offset: Vec2,
    canvas: xilem::masonry::kurbo::Rect,
    original: Vec<MonitorConfig>,
    moved: bool,
}

impl MonitorLayoutWidget {
    pub(crate) fn new(monitors: Vec<MonitorConfig>, selected_monitor_id: Option<String>) -> Self {
        Self {
            monitors,
            selected_monitor_id,
            drag: None,
        }
    }

    pub(crate) fn set_monitors(
        &mut self,
        monitors: Vec<MonitorConfig>,
        selected_monitor_id: Option<String>,
        ctx: &mut MutateCtx<'_>,
    ) {
        if self.monitors != monitors || self.selected_monitor_id != selected_monitor_id {
            if self.drag.is_some()
                && (self.monitors.len() != monitors.len()
                    || self.monitors.iter().zip(&monitors).any(|(old, new)| {
                        old.id != new.id
                            || old.width != new.width
                            || old.height != new.height
                            || old.scale != new.scale
                    }))
            {
                self.drag = None;
            }
            self.monitors = monitors;
            self.selected_monitor_id = selected_monitor_id;
            ctx.request_render();
            ctx.request_accessibility_update();
        }
    }

    fn layout_metrics(&self, size: Size) -> LayoutMetrics {
        self.drag.as_ref().map_or_else(
            || LayoutMetrics::new(&self.monitors, size),
            |drag| LayoutMetrics::from_canvas(drag.canvas, size),
        )
    }

    fn hit_monitor(&self, point: Point, metrics: LayoutMetrics) -> Option<&MonitorConfig> {
        if !metrics.contains(point) {
            return None;
        }
        self.monitors
            .iter()
            .filter(|monitor| self.selected_monitor_id.as_ref() == Some(&monitor.id))
            .chain(
                self.monitors
                    .iter()
                    .rev()
                    .filter(|monitor| self.selected_monitor_id.as_ref() != Some(&monitor.id)),
            )
            .find(|monitor| metrics.rect(monitor).contains(point))
    }

    fn monitors_for_drag(
        &self,
        drag: &MonitorDrag,
        point: Point,
        size: Size,
        normalize: bool,
    ) -> Vec<MonitorConfig> {
        let metrics = LayoutMetrics::from_canvas(drag.canvas, size);
        let origin = metrics.world_point(point) - drag.grab_offset;
        let (x, y) = metrics.bounded_position(
            &self.monitors,
            &drag.id,
            origin.x.round() as i32,
            origin.y.round() as i32,
        );
        // The reference allows free placement while held, then joins a neighbor
        // and recenters on release. A bounded live preview must not forbid moving
        // to the other side of an edge display.
        let (x, y) = if normalize {
            snap_monitor_to_neighbor(&self.monitors, &drag.id, x, y)
        } else {
            (x, y)
        };
        let mut monitors = self.monitors.clone();
        set_monitor_position(&mut monitors, &drag.id, x, y);
        if normalize {
            normalize_monitors_near_origin(&mut monitors);
        }
        monitors
    }
}

impl Widget for MonitorLayoutWidget {
    type Action = MonitorEditorAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: None | Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let point = ctx.local_position(state.position);
                let metrics = self.layout_metrics(ctx.content_box().size());
                if let Some(monitor) = self.hit_monitor(point, metrics) {
                    let selected_id = monitor.id.clone();
                    let grab_offset = metrics.world_point(point)
                        - Point::new(f64::from(monitor.x), f64::from(monitor.y));
                    ctx.capture_pointer();
                    ctx.request_focus();
                    ctx.set_handled();
                    self.selected_monitor_id = Some(selected_id.clone());
                    self.drag = Some(MonitorDrag {
                        id: selected_id.clone(),
                        start_pointer: point,
                        grab_offset,
                        canvas: metrics.canvas,
                        original: self.monitors.clone(),
                        moved: false,
                    });
                    ctx.submit_action::<MonitorEditorAction>(MonitorEditorAction::Select {
                        id: selected_id,
                        monitors: self.monitors.clone(),
                    });
                    ctx.request_render();
                }
            }
            PointerEvent::Move(PointerUpdate { current, .. }) if ctx.is_active() => {
                if let Some(drag) = &mut self.drag {
                    let point = ctx.local_position(current.position);
                    if !drag.moved && point.distance(drag.start_pointer) < DRAG_START_DISTANCE {
                        return;
                    }
                    drag.moved = true;
                    // A resized board can expose more world area. Freeze that
                    // full area before moving into it, so later resizes still
                    // fit every tile without changing desktop coordinates.
                    drag.canvas =
                        LayoutMetrics::from_canvas(drag.canvas, ctx.content_box().size()).canvas;
                    let drag = drag.clone();
                    let moved_id = drag.id.clone();
                    let monitors =
                        self.monitors_for_drag(&drag, point, ctx.content_box().size(), false);
                    self.monitors = monitors.clone();
                    ctx.submit_action::<MonitorEditorAction>(MonitorEditorAction::Drag {
                        moved_id,
                        monitors,
                        commit: false,
                    });
                    ctx.request_render();
                    ctx.set_handled();
                }
            }
            PointerEvent::Up(PointerButtonEvent {
                button: None | Some(PointerButton::Primary),
                state,
                ..
            }) => {
                if let Some(drag) = self.drag.take() {
                    if !drag.moved {
                        self.selected_monitor_id = Some(drag.id.clone());
                        ctx.submit_action::<MonitorEditorAction>(MonitorEditorAction::Select {
                            id: drag.id,
                            monitors: self.monitors.clone(),
                        });
                        ctx.request_render();
                        ctx.set_handled();
                        return;
                    }
                    let point = ctx.local_position(state.position);
                    let monitors =
                        self.monitors_for_drag(&drag, point, ctx.content_box().size(), true);
                    self.monitors = monitors.clone();
                    ctx.submit_action::<MonitorEditorAction>(MonitorEditorAction::Drag {
                        moved_id: drag.id,
                        monitors,
                        commit: true,
                    });
                    ctx.request_render();
                    ctx.set_handled();
                }
            }
            PointerEvent::Cancel(..) => {
                if let Some(drag) = self.drag.take() {
                    self.monitors = drag.original;
                    ctx.submit_action::<MonitorEditorAction>(MonitorEditorAction::Select {
                        id: drag.id,
                        monitors: self.monitors.clone(),
                    });
                    ctx.request_render();
                }
            }
            _ => {}
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        self.handle_key(ctx, event);
    }

    fn on_access_event(
        &mut self,
        _: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        _: &AccessEvent,
    ) {
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::FocusChanged(_)) {
            ctx.request_paint_only();
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        match axis {
            Axis::Horizontal => 400.0.px(),
            Axis::Vertical => 225.0.px(),
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        paint::preview(self, ctx, painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::Canvas
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label("Drag monitor rectangles to configure their arrangement");
        let selected = self
            .monitors
            .iter()
            .enumerate()
            .find(|(_, monitor)| self.selected_monitor_id.as_ref() == Some(&monitor.id));
        node.set_description(selected.map_or_else(|| "No display selected.".into(), |(index, monitor)| format!("Display {}: {}. Space selects the next display. Arrow keys place it beside another display.", index+1, monitor.name)));
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, pos: Point) -> CursorIcon {
        let metrics = self.layout_metrics(ctx.content_box().size());
        if self.hit_monitor(ctx.to_local(pos), metrics).is_some() {
            CursorIcon::Grab
        } else {
            CursorIcon::Default
        }
    }
}

#[cfg(test)]
mod edge_tests;
#[cfg(test)]
mod tests;
