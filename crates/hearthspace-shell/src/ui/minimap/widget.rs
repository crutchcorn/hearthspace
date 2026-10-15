use xilem::{
    Pod, ViewCtx,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
            PointerEvent, PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, TextEvent, Widget,
            WidgetId,
            pointer::{PointerButton, PointerId},
        },
        imaging::Painter,
        kurbo::{Axis, Point, Rect, Size},
        layout::{LenReq, Length},
    },
};

use super::{CanvasSnapshot, Projection, description, render};
use crate::{
    ShellCommand,
    ui::{command_client::send_command, state::ShellState},
};

#[derive(Debug, PartialEq)]
pub(super) enum MapAction {
    Begin,
    PanTo { x: i32, y: i32 },
    End,
}

pub(super) struct MinimapView {
    pub(super) snapshot: Option<CanvasSnapshot>,
    pub(super) output_name: Option<String>,
}

impl ViewMarker for MinimapView {}

impl View<ShellState, (), ViewCtx> for MinimapView {
    type Element = Pod<Minimap>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut ShellState) -> (Self::Element, ()) {
        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(Minimap::new(
                    self.snapshot.clone(),
                    self.output_name.clone(),
                ))
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut ShellState,
    ) {
        if self.snapshot != prev.snapshot || self.output_name != prev.output_name {
            element.widget.snapshot = self.snapshot.clone();
            element.widget.output_name = self.output_name.clone();
            element.ctx.request_render();
            element.ctx.request_accessibility_update();
        }
    }

    fn teardown(&self, _: &mut (), _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        _: &mut (),
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        match message.take_message::<MapAction>().map(|action| *action) {
            Some(MapAction::Begin) => state.canvas_dragging = true,
            Some(MapAction::End) => state.canvas_dragging = false,
            Some(MapAction::PanTo { x, y }) => {
                match send_command(
                    &state.command_socket,
                    &ShellCommand::PanTo { x, y }.wire_name(),
                ) {
                    Ok(()) => {
                        state
                            .canvas_revision
                            .fetch_add(1, std::sync::atomic::Ordering::Release);
                        if let Some(snapshot) = &mut state.canvas {
                            snapshot.set_offset(x, y);
                        }
                        state.canvas_error = None;
                    }
                    Err(error) => {
                        state.canvas_error = Some(error);
                        state.canvas_dragging = false;
                        element.widget.drag = None;
                        element.widget.snapshot = state.canvas.clone();
                        element.ctx.request_render();
                        element.ctx.request_cursor_icon_change();
                    }
                }
            }
            None => return MessageResult::Stale,
        }
        MessageResult::Action(())
    }
}

#[derive(Clone, Debug)]
struct Drag {
    projection: Projection,
    pointer: Option<PointerId>,
    start: Point,
    offset: (f64, f64),
}

pub(super) struct Minimap {
    snapshot: Option<CanvasSnapshot>,
    output_name: Option<String>,
    size: Size,
    drag: Option<Drag>,
}

impl Minimap {
    pub(super) fn new(snapshot: Option<CanvasSnapshot>, output_name: Option<String>) -> Self {
        Self {
            snapshot,
            output_name,
            size: Size::ZERO,
            drag: None,
        }
    }

    fn viewport(&self, projection: &Projection) -> Option<Rect> {
        let snapshot = self.snapshot.as_ref()?;
        let output = match &self.output_name {
            Some(name) => snapshot.outputs.iter().find(|output| &output.name == name),
            None => snapshot.outputs.first(),
        }?;
        Some(projection.rect(snapshot.output_bounds(output)))
    }

    fn begin(&mut self, point: Point, pointer: Option<PointerId>) -> bool {
        if self.drag.is_some() || self.size.width <= 28.0 || self.size.height <= 28.0 {
            return false;
        }
        let projection = Projection::new(self.snapshot.as_ref(), self.size);
        if !self
            .viewport(&projection)
            .is_some_and(|rect| rect.inflate(3.0, 3.0).contains(point))
        {
            return false;
        }
        self.drag = Some(Drag {
            projection,
            pointer,
            start: point,
            offset: self.snapshot.as_ref().unwrap().offset,
        });
        true
    }

    fn pan(&mut self, point: Point) -> Option<MapAction> {
        let drag = self.drag.as_ref()?;
        let x = (drag.offset.0 + (point.x - drag.start.x) / drag.projection.scale)
            .round()
            .clamp(-1e6, 1e6) as i32;
        let y = (drag.offset.1 + (point.y - drag.start.y) / drag.projection.scale)
            .round()
            .clamp(-1e6, 1e6) as i32;
        let snapshot = self.snapshot.as_mut()?;
        if snapshot.offset == (f64::from(x), f64::from(y)) {
            return None;
        }
        snapshot.set_offset(x, y);
        Some(MapAction::PanTo { x, y })
    }

    fn finish(&mut self, ctx: &mut EventCtx<'_>) {
        if self.drag.take().is_some() {
            ctx.submit_action::<MapAction>(MapAction::End);
            ctx.request_render();
            ctx.request_cursor_icon_change();
        }
    }
}

impl Widget for Minimap {
    type Action = MapAction;

    fn register_children(&mut self, _: &mut RegisterCtx<'_>) {}

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(event)
                if !ctx.is_disabled()
                    && matches!(event.button, None | Some(PointerButton::Primary)) =>
            {
                if self.begin(
                    ctx.local_position(event.state.position),
                    event.pointer.pointer_id,
                ) {
                    ctx.capture_pointer();
                    ctx.submit_action::<MapAction>(MapAction::Begin);
                    ctx.request_render();
                    ctx.request_cursor_icon_change();
                    ctx.set_handled();
                }
            }
            PointerEvent::Move(event)
                if self
                    .drag
                    .as_ref()
                    .is_some_and(|drag| drag.pointer == event.pointer.pointer_id) =>
            {
                if let Some(action) = self.pan(ctx.local_position(event.current.position)) {
                    ctx.submit_action::<MapAction>(action);
                    ctx.request_render();
                    ctx.request_accessibility_update();
                }
                ctx.set_handled();
            }
            PointerEvent::Up(event) if self.drag.is_some() => {
                if self
                    .drag
                    .as_ref()
                    .is_some_and(|drag| drag.pointer == event.pointer.pointer_id)
                    && matches!(event.button, None | Some(PointerButton::Primary))
                    && let Some(action) = self.pan(ctx.local_position(event.state.position))
                {
                    ctx.submit_action::<MapAction>(action);
                }
                // Masonry releases capture after every Up, including another
                // button/contact's release. End here too rather than leaving
                // a drag that can continue later without pointer capture.
                self.finish(ctx);
                ctx.set_handled();
            }
            PointerEvent::Cancel(_) => self.finish(ctx),
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if matches!(event, TextEvent::WindowFocusChange(false)) && self.drag.is_some() {
            if ctx.is_active() {
                ctx.release_pointer();
            }
            self.finish(ctx);
        }
    }

    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        _: Option<Length>,
    ) -> Length {
        match request {
            LenReq::FitContent(space) => space,
            _ => Length::const_px(if axis == Axis::Horizontal {
                312.0
            } else {
                190.0
            }),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        render::draw(
            painter,
            self.size,
            self.snapshot.as_ref(),
            self.output_name.as_deref(),
            self.drag.as_ref().map(|drag| &drag.projection),
        );
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, point: Point) -> CursorIcon {
        if self.drag.is_some() {
            return CursorIcon::Grabbing;
        }
        let map = Projection::new(self.snapshot.as_ref(), self.size);
        if self
            .viewport(&map)
            .is_some_and(|rect| rect.inflate(3.0, 3.0).contains(ctx.to_local(point)))
        {
            CursorIcon::Grab
        } else {
            CursorIcon::Default
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Image
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(description(
            self.snapshot.as_ref(),
            self.output_name.as_deref(),
        ));
        node.set_description("Drag the blue viewport rectangle to pan the canvas.");
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("Minimap", id = id.trace())
    }
}

#[cfg(test)]
#[path = "widget_tests.rs"]
mod tests;
