use hearthspace_ipc::MonitorConfig;
use hearthspace_ui_components as components;
use xilem::masonry::accesskit::{Node, Role};
use xilem::masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef,
    QueryCtx, RegisterCtx, TextEvent, Widget,
};
use xilem::masonry::imaging::Painter;
use xilem::masonry::kurbo::{Axis, Point, Rect, Size, Stroke};
use xilem::masonry::layout::{LenReq, Length};
use xilem::masonry::{core::MutateCtx, layout::AsUnit};

use crate::{
    actions::MonitorEditorAction,
    monitors::{
        EMPTY_MONITOR_CANVAS_HEIGHT, EMPTY_MONITOR_CANVAS_WIDTH, monitor_logical_height,
        monitor_logical_width,
    },
};

const EDITOR_PADDING: f64 = 24.0;
const MONITOR_SNAP: i32 = 10;
const MONITOR_LAYOUT_CANVAS_MULTIPLIER: f64 = 1.2;
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
    start_x: i32,
    start_y: i32,
    origin_x: f64,
    origin_y: f64,
    scale: f64,
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
            self.monitors = monitors;
            self.selected_monitor_id = selected_monitor_id;
            ctx.request_render();
        }
    }

    fn layout_metrics(&self, size: Size) -> LayoutMetrics {
        if let Some(drag) = &self.drag {
            return LayoutMetrics {
                origin_x: drag.origin_x,
                origin_y: drag.origin_y,
                scale: drag.scale,
            };
        }

        let canvas_size = monitor_canvas_size(&self.monitors);
        let scale = layout_scale_for_canvas(&self.monitors, size);
        let (origin_x, origin_y) = monitor_canvas_origin(&self.monitors, canvas_size);

        LayoutMetrics {
            origin_x,
            origin_y,
            scale,
        }
    }

    fn monitor_rect(&self, monitor: &MonitorConfig, metrics: LayoutMetrics) -> Rect {
        Rect::new(
            EDITOR_PADDING + (f64::from(monitor.x) - metrics.origin_x) * metrics.scale,
            EDITOR_PADDING + (f64::from(monitor.y) - metrics.origin_y) * metrics.scale,
            EDITOR_PADDING
                + (f64::from(monitor.x + monitor_logical_width(monitor)) - metrics.origin_x)
                    * metrics.scale,
            EDITOR_PADDING
                + (f64::from(monitor.y + monitor_logical_height(monitor)) - metrics.origin_y)
                    * metrics.scale,
        )
    }

    fn hit_monitor(&self, point: Point, metrics: LayoutMetrics) -> Option<&MonitorConfig> {
        self.monitors
            .iter()
            .rev()
            .find(|monitor| self.monitor_rect(monitor, metrics).contains(point))
    }

    fn monitors_for_drag(
        &self,
        drag: &MonitorDrag,
        point: Point,
        normalize: bool,
    ) -> Vec<MonitorConfig> {
        let x = drag.start_x + ((point.x - drag.start_pointer.x) / drag.scale).round() as i32;
        let y = drag.start_y + ((point.y - drag.start_pointer.y) / drag.scale).round() as i32;
        let (x, y) =
            snap_monitor_to_neighbor(&self.monitors, &drag.id, snap_position(x), snap_position(y));
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
                    let start_x = monitor.x;
                    let start_y = monitor.y;
                    ctx.capture_pointer();
                    ctx.request_focus();
                    ctx.set_handled();
                    self.selected_monitor_id = Some(selected_id.clone());
                    self.drag = Some(MonitorDrag {
                        id: selected_id.clone(),
                        start_pointer: point,
                        start_x,
                        start_y,
                        origin_x: metrics.origin_x,
                        origin_y: metrics.origin_y,
                        scale: metrics.scale,
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
                    let drag = drag.clone();
                    let moved_id = drag.id.clone();
                    let monitors = self.monitors_for_drag(&drag, point, false);
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
                    let monitors = self.monitors_for_drag(&drag, point, true);
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
            PointerEvent::Cancel(..) => self.drag = None,
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &TextEvent,
    ) {
    }

    fn on_access_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &AccessEvent,
    ) {
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
            Axis::Horizontal => 520.0.px(),
            Axis::Vertical => 300.0.px(),
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let content = ctx.content_box();
        painter.fill(content, components::SURFACE_SUBTLE).draw();

        let metrics = self.layout_metrics(content.size());
        for monitor in &self.monitors {
            let rect = self.monitor_rect(monitor, metrics);
            let active = self.drag.as_ref().is_some_and(|drag| drag.id == monitor.id);
            let selected = self
                .selected_monitor_id
                .as_ref()
                .is_some_and(|id| id == &monitor.id);
            let fill = if active {
                components::ACCENT
            } else if selected {
                components::ACCENT_SUBTLE
            } else {
                components::SURFACE
            };
            painter.fill(rect, fill).draw();
            painter
                .stroke(
                    rect,
                    &Stroke::new(2.0),
                    if selected {
                        components::ACCENT
                    } else {
                        components::BORDER
                    },
                )
                .draw();
        }
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

#[derive(Clone, Copy)]
struct LayoutMetrics {
    origin_x: f64,
    origin_y: f64,
    scale: f64,
}

fn monitor_canvas_size(monitors: &[MonitorConfig]) -> Size {
    let (width, height) = monitor_canvas_span(monitors);
    Size::new(
        f64::from(width) * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
        f64::from(height) * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
    )
}

fn monitor_canvas_origin(monitors: &[MonitorConfig], canvas_size: Size) -> (f64, f64) {
    let Some((min_x, min_y, max_x, max_y)) = monitor_arrangement_bounds(monitors) else {
        return (canvas_size.width / -2.0, canvas_size.height / -2.0);
    };
    let center_x = (f64::from(min_x) + f64::from(max_x)) / 2.0;
    let center_y = (f64::from(min_y) + f64::from(max_y)) / 2.0;
    (
        center_x - canvas_size.width / 2.0,
        center_y - canvas_size.height / 2.0,
    )
}

fn monitor_canvas_span(monitors: &[MonitorConfig]) -> (i32, i32) {
    if monitors.is_empty() {
        return (EMPTY_MONITOR_CANVAS_WIDTH, EMPTY_MONITOR_CANVAS_HEIGHT);
    }

    (
        combined_monitor_width(monitors),
        combined_monitor_height(monitors),
    )
}

fn combined_monitor_width(monitors: &[MonitorConfig]) -> i32 {
    monitors.iter().map(monitor_logical_width).sum()
}

fn combined_monitor_height(monitors: &[MonitorConfig]) -> i32 {
    monitors.iter().map(monitor_logical_height).sum()
}

fn layout_scale_for_canvas(monitors: &[MonitorConfig], size: Size) -> f64 {
    let canvas_size = monitor_canvas_size(monitors);
    let available_width = (size.width - EDITOR_PADDING * 2.0).max(1.0);
    let available_height = (size.height - EDITOR_PADDING * 2.0).max(1.0);
    (available_width / canvas_size.width)
        .min(available_height / canvas_size.height)
        .clamp(0.03, 1.0)
}

fn set_monitor_position(monitors: &mut [MonitorConfig], id: &str, x: i32, y: i32) {
    if let Some(monitor) = monitors.iter_mut().find(|monitor| monitor.id == id) {
        monitor.x = x;
        monitor.y = y;
    }
}

fn normalize_monitors_near_origin(monitors: &mut [MonitorConfig]) {
    let Some((min_x, min_y, max_x, max_y)) = monitor_arrangement_bounds(monitors) else {
        return;
    };
    let center_x = midpoint_i32(min_x, max_x);
    let center_y = midpoint_i32(min_y, max_y);
    for monitor in monitors {
        monitor.x -= center_x;
        monitor.y -= center_y;
    }
}

fn monitor_arrangement_bounds(monitors: &[MonitorConfig]) -> Option<(i32, i32, i32, i32)> {
    let first = monitors.first()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x + monitor_logical_width(first);
    let mut max_y = first.y + monitor_logical_height(first);
    for monitor in monitors.iter().skip(1) {
        min_x = min_x.min(monitor.x);
        min_y = min_y.min(monitor.y);
        max_x = max_x.max(monitor.x + monitor_logical_width(monitor));
        max_y = max_y.max(monitor.y + monitor_logical_height(monitor));
    }
    Some((min_x, min_y, max_x, max_y))
}

fn midpoint_i32(start: i32, end: i32) -> i32 {
    ((i64::from(start) + i64::from(end)) / 2) as i32
}

fn snap_position(value: i32) -> i32 {
    let half = MONITOR_SNAP / 2;
    if value >= 0 {
        ((value + half) / MONITOR_SNAP) * MONITOR_SNAP
    } else {
        ((value - half) / MONITOR_SNAP) * MONITOR_SNAP
    }
}

fn snap_monitor_to_neighbor(monitors: &[MonitorConfig], id: &str, x: i32, y: i32) -> (i32, i32) {
    let Some(monitor) = monitors.iter().find(|monitor| monitor.id == id) else {
        return (x, y);
    };
    let candidates = monitors
        .iter()
        .filter(|other| other.id != id)
        .flat_map(|other| bordered_positions_for(monitor, other, x, y));

    candidates
        .min_by_key(|(candidate_x, candidate_y)| {
            let dx = i64::from(*candidate_x - x);
            let dy = i64::from(*candidate_y - y);
            dx * dx + dy * dy
        })
        .unwrap_or((x, y))
}

fn bordered_positions_for(
    monitor: &MonitorConfig,
    other: &MonitorConfig,
    x: i32,
    y: i32,
) -> [(i32, i32); 4] {
    let monitor_width = monitor_logical_width(monitor);
    let monitor_height = monitor_logical_height(monitor);
    let other_width = monitor_logical_width(other);
    let other_height = monitor_logical_height(other);
    let left_of_other = other.x - monitor_width;
    let right_of_other = other.x + other_width;
    let above_other = other.y - monitor_height;
    let below_other = other.y + other_height;

    [
        (
            left_of_other,
            clamp_overlap(y, monitor_height, other.y, other_height),
        ),
        (
            right_of_other,
            clamp_overlap(y, monitor_height, other.y, other_height),
        ),
        (
            clamp_overlap(x, monitor_width, other.x, other_width),
            above_other,
        ),
        (
            clamp_overlap(x, monitor_width, other.x, other_width),
            below_other,
        ),
    ]
}

fn clamp_overlap(position: i32, size: i32, other_position: i32, other_size: i32) -> i32 {
    let size = size.max(1);
    let other_size = other_size.max(1);
    position.clamp(other_position - size + 1, other_position + other_size - 1)
}

#[cfg(test)]
mod tests;
