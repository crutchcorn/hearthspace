use hearthspace_ui_components as components;
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ChildrenIds, ComposeCtx, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
        PointerEvent, PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, TextEvent, Update,
        UpdateCtx, Widget,
        keyboard::{Key, NamedKey},
        pointer::{PointerButton, PointerId},
    },
    imaging::Painter,
    kurbo::{Axis, BezPath, Cap, Circle, Join, Point, Size, Stroke},
    layout::{LenReq, Length},
};

const MAX_POINTS: usize = 2048;

#[derive(Debug, PartialEq)]
pub enum DrawingAction {
    Stroke { context: u64, points: Vec<[f64; 2]> },
    Bounds { context: u64, rect: [f64; 4] },
}

pub(super) struct DrawingStroke {
    pointer: Option<PointerId>,
    points: Vec<[f64; 2]>,
}

pub struct DrawingPad {
    pub(super) context: u64,
    pub(super) capture_locally: bool,
    pub(super) status: String,
    pub(super) stroke: Option<DrawingStroke>,
    pub(super) last_bounds: Option<[f64; 4]>,
    preview: Vec<[f64; 2]>,
    size: Size,
}

impl DrawingPad {
    pub(super) fn new(
        context: u64,
        capture_locally: bool,
        status: String,
        preview: Vec<[f64; 2]>,
    ) -> Self {
        let mut result = Self {
            context,
            capture_locally,
            status,
            stroke: None,
            last_bounds: None,
            preview: vec![],
            size: Size::ZERO,
        };
        result.set_preview(&preview);
        result
    }

    pub(super) fn set_preview(&mut self, points: &[[f64; 2]]) {
        if !self.capture_locally && !points.is_empty() {
            self.stroke = None;
        }
        self.preview = points
            .iter()
            .take(MAX_POINTS)
            .filter(|point| {
                point
                    .iter()
                    .all(|value| value.is_finite() && value.abs() <= 1e6)
            })
            .copied()
            .collect();
    }

    fn contains(&self, point: Point) -> bool {
        point.is_finite() && self.size.to_rect().contains(point)
    }

    fn sample(&mut self, point: Point) -> bool {
        if !self.contains(point) {
            return false;
        }
        let Some(stroke) = &mut self.stroke else {
            return false;
        };
        if stroke.points.last().is_some_and(|previous| {
            Point::new(previous[0], previous[1]).distance_squared(point) < 0.25
        }) {
            return true;
        }
        // Reject the whole overlong stroke; truncating could launch the app
        // matching its prefix even though the user drew a different shape.
        if stroke.points.len() >= MAX_POINTS {
            return false;
        }
        stroke.points.push([point.x, point.y]);
        true
    }

    fn cancel(&mut self, ctx: &mut EventCtx<'_>) {
        if self.stroke.take().is_some() {
            if ctx.is_active() {
                ctx.release_pointer();
            }
            ctx.request_render();
            ctx.request_accessibility_update();
        }
    }

    fn points(&self) -> &[[f64; 2]] {
        self.stroke
            .as_ref()
            .map_or(&self.preview, |stroke| &stroke.points)
    }
}

impl Widget for DrawingPad {
    type Action = DrawingAction;

    fn accepts_focus(&self) -> bool {
        self.capture_locally
    }

    fn register_children(&mut self, _: &mut RegisterCtx<'_>) {}

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(event) => {
                if self.stroke.is_some() {
                    self.cancel(ctx);
                    ctx.set_handled();
                } else if !ctx.is_disabled()
                    && ctx.is_window_focused()
                    && self.capture_locally
                    && matches!(event.button, None | Some(PointerButton::Primary))
                    && self.contains(ctx.local_position(event.state.position))
                {
                    self.stroke = Some(DrawingStroke {
                        pointer: event.pointer.pointer_id,
                        points: vec![],
                    });
                    self.sample(ctx.local_position(event.state.position));
                    ctx.capture_pointer();
                    ctx.request_focus();
                    ctx.request_render();
                    ctx.request_accessibility_update();
                    ctx.set_handled();
                }
            }
            PointerEvent::Move(event) if self.stroke.is_some() => {
                if self.stroke.as_ref().unwrap().pointer != event.pointer.pointer_id {
                    self.cancel(ctx);
                } else {
                    for sample in event
                        .coalesced
                        .iter()
                        .chain(std::iter::once(&event.current))
                    {
                        if !self.sample(ctx.local_position(sample.position)) {
                            self.cancel(ctx);
                            break;
                        }
                    }
                    ctx.request_render();
                    ctx.request_accessibility_update();
                }
                ctx.set_handled();
            }
            PointerEvent::Up(event) if self.stroke.is_some() => {
                let valid = self.stroke.as_ref().unwrap().pointer == event.pointer.pointer_id
                    && matches!(event.button, None | Some(PointerButton::Primary))
                    && self.sample(ctx.local_position(event.state.position));
                if valid {
                    let points = self.stroke.take().unwrap().points;
                    ctx.submit_action::<DrawingAction>(DrawingAction::Stroke {
                        context: self.context,
                        points,
                    });
                    ctx.request_render();
                    ctx.request_accessibility_update();
                } else {
                    self.cancel(ctx);
                }
                ctx.set_handled();
            }
            PointerEvent::Cancel(_) => self.cancel(ctx),
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let escape = matches!(event, TextEvent::Keyboard(key)
            if key.state.is_down() && key.key == Key::Named(NamedKey::Escape));
        if escape || matches!(event, TextEvent::WindowFocusChange(false)) {
            self.cancel(ctx);
        }
        // Do not swallow Escape: the enclosing search mode can close as well.
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(
            event,
            Update::FocusChanged(false)
                | Update::DisabledChanged(true)
                | Update::StashedChanged(true)
                | Update::ActiveChanged(false)
        ) && self.stroke.take().is_some()
        {
            ctx.request_render();
            ctx.request_accessibility_update();
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
                440.0
            } else {
                220.0
            }),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        if self.size != size {
            self.stroke = None;
        }
        self.size = size;
        ctx.set_clip_path(size.to_rect());
    }

    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        let rect = ctx
            .window_transform()
            .transform_rect_bbox(self.size.to_rect());
        let rect = [rect.x0, rect.y0, rect.width(), rect.height()];
        if self.last_bounds != Some(rect) && rect.iter().all(|value| value.is_finite()) {
            self.last_bounds = Some(rect);
            let context = self.context;
            ctx.mutate_self_later(move |mut widget| {
                widget
                    .ctx
                    .submit_action::<DrawingAction>(DrawingAction::Bounds { context, rect });
            });
        }
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let bounds = self.size.to_rect().inset(-1.0).to_rounded_rect(8.0);
        painter.fill(bounds, components::SURFACE_SUBTLE).draw();
        let border = if ctx.is_focus_target() {
            components::ACCENT
        } else {
            components::BORDER
        };
        painter.stroke(bounds, &Stroke::new(1.0), border).draw();
        // A quiet grid indicates the active drawing region without implying a
        // required starting point, size or exact tracing path.
        for x in (20..self.size.width.max(0.0) as usize).step_by(24) {
            for y in (20..self.size.height.max(0.0) as usize).step_by(24) {
                painter
                    .fill(Circle::new((x as f64, y as f64), 0.8), components::BORDER)
                    .draw();
            }
        }
        let mut path = BezPath::new();
        for (index, point) in self.points().iter().enumerate() {
            if index == 0 {
                path.move_to(Point::new(point[0], point[1]));
            } else {
                path.line_to(Point::new(point[0], point[1]));
            }
        }
        painter
            .stroke(
                path,
                &Stroke::new(3.0)
                    .with_caps(Cap::Round)
                    .with_join(Join::Round),
                components::ACCENT,
            )
            .draw();
        if let Some(point) = self.points().first() {
            painter
                .fill(
                    Circle::new(Point::new(point[0], point[1]), 3.5),
                    components::ACCENT,
                )
                .draw();
        }
    }

    fn get_cursor(&self, _: &QueryCtx<'_>, _: Point) -> CursorIcon {
        CursorIcon::Crosshair
    }

    fn accessibility_role(&self) -> Role {
        Role::Canvas
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label("Gesture drawing area");
        node.set_description(
            "Draw one continuous shape inside this area, then lift to finish. Escape cancels.",
        );
        node.set_value(format!("{}; {} points", self.status, self.points().len()));
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}
