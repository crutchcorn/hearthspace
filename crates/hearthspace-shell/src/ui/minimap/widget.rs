use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
        PointerEvent, PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, TextEvent, Update,
        UpdateCtx, Widget, WidgetId,
        pointer::{PointerButton, PointerId},
    },
    imaging::Painter,
    kurbo::{Axis, Point, Rect, Size},
    layout::{LenReq, Length},
};

use super::{CanvasSnapshot, Projection, description, render};
use hearthspace_ui_components::settings_material::FocusIndicator;

#[derive(Debug, PartialEq)]
pub(super) enum MapAction {
    Begin,
    PanTo { x: i32, y: i32 },
    End,
}

#[derive(Clone, Debug)]
pub(super) struct Drag {
    projection: Projection,
    pointer: Option<PointerId>,
    start: Point,
    offset: (f64, f64),
}

pub(super) struct Minimap {
    pub(super) snapshot: Option<CanvasSnapshot>,
    pub(super) output_name: Option<String>,
    size: Size,
    pub(super) drag: Option<Drag>,
    focus: FocusIndicator,
}

impl Minimap {
    pub(super) fn new(snapshot: Option<CanvasSnapshot>, output_name: Option<String>) -> Self {
        Self {
            snapshot,
            output_name,
            size: Size::ZERO,
            drag: None,
            focus: FocusIndicator::default(),
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
        let Some(viewport) = self.viewport(&projection) else {
            return false;
        };
        // Presses inside the viewport retain their grab offset. Everywhere else
        // uses its center as the anchor, including outputs with negative origins.
        let start = if viewport.inflate(1.0, 1.0).contains(point) {
            point
        } else {
            viewport.center()
        };
        self.drag = Some(Drag {
            projection,
            pointer,
            start,
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
        self.focus.pointer(event);
        ctx.request_paint_only();
        match event {
            PointerEvent::Down(event)
                if !ctx.is_disabled()
                    && matches!(event.button, None | Some(PointerButton::Primary)) =>
            {
                let point = ctx.local_position(event.state.position);
                if self.begin(point, event.pointer.pointer_id) {
                    ctx.capture_pointer();
                    ctx.request_focus();
                    ctx.submit_action::<MapAction>(MapAction::Begin);
                    if self.drag.as_ref().is_some_and(|drag| drag.start != point)
                        && let Some(action) = self.pan(point)
                    {
                        ctx.submit_action::<MapAction>(action);
                        ctx.request_accessibility_update();
                    }
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
            PointerEvent::Cancel(_) if self.drag.is_some() => {
                // Even an unrelated cancellation drops Masonry's capture.
                self.finish(ctx);
                ctx.set_handled();
            }
            _ if self.drag.is_some() => ctx.set_handled(),
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        self.focus.text(event);
        ctx.request_paint_only();
        if matches!(event, TextEvent::WindowFocusChange(false)) && self.drag.is_some() {
            if ctx.is_active() {
                ctx.release_pointer();
            }
            self.finish(ctx);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        self.focus.update(event);
        if matches!(event, Update::FocusChanged(false)) && self.drag.take().is_some() {
            ctx.submit_action::<MapAction>(MapAction::End);
            ctx.request_cursor_icon_change();
        }
        if matches!(
            event,
            Update::FocusChanged(_) | Update::HoveredChanged(_) | Update::DisabledChanged(_)
        ) {
            ctx.request_paint_only();
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
            LenReq::MinContent if axis == Axis::Horizontal => Length::const_px(28.0),
            LenReq::FitContent(space) => space,
            _ => Length::const_px(if axis == Axis::Horizontal {
                310.0
            } else {
                168.0
            }),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        // The renderer clips map content, leaving room for the exterior focus
        // ring and glow without expanding hit bounds or projection coordinates.
        ctx.set_paint_insets(24.0);
    }

    fn property_changed(
        &mut self,
        ctx: &mut xilem::masonry::core::UpdateCtx<'_>,
        property: std::any::TypeId,
    ) {
        hearthspace_ui_components::Palette::prop_changed(ctx, property);
    }
    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let palette = *props.get::<hearthspace_ui_components::Palette>(ctx.property_cache());
        render::draw(
            painter,
            self.size,
            self.snapshot.as_ref(),
            self.output_name.as_deref(),
            self.drag.as_ref().map(|drag| &drag.projection),
            render::Interaction {
                emphasized: !ctx.is_disabled()
                    && (ctx.is_hovered() || ctx.is_focus_target() || self.drag.is_some()),
                focused: !ctx.is_disabled() && ctx.is_focus_target() && self.focus.visible(),
            },
            &palette,
        );
    }

    fn get_cursor(&self, _: &QueryCtx<'_>, _: Point) -> CursorIcon {
        if self.drag.is_some() {
            return CursorIcon::Grabbing;
        }
        CursorIcon::Grab
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Role {
        Role::Image
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(description(
            self.snapshot.as_ref(),
            self.output_name.as_deref(),
        ));
        node.set_description("Drag the accented viewport to pan the canvas. Press elsewhere on the map to center the viewport there, then drag.");
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
