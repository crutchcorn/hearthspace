use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ActionCtx, ChildrenIds, ErasedAction, EventCtx, FromDynWidget, LayoutCtx,
        MeasureCtx, NewWidget, NoAction, PaintCtx, PointerEvent, PointerType, PropertiesMut,
        PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut,
        WidgetPod, pointer::PointerId,
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size},
    layout::{LayoutSize, LenReq, Length},
    widgets::{ButtonPress, Portal},
};

const DRAG_THRESHOLD: f64 = 8.0;

struct Contact {
    pointer: Option<PointerId>,
    start: Point,
    previous: Point,
    button: Option<WidgetId>,
    allowed: bool,
    dragging: bool,
}

#[derive(Debug)]
struct EndTouch;

pub struct TouchScroll<W: Widget + FromDynWidget + ?Sized> {
    portal: WidgetPod<Portal<W>>,
    pub(super) horizontal: bool,
    pub(super) vertical: bool,
    contact: Option<Contact>,
    suppressed_button: Option<WidgetId>,
    generation: u64,
}

impl<W: Widget + FromDynWidget + ?Sized> TouchScroll<W> {
    pub(super) fn new(portal: NewWidget<Portal<W>>, horizontal: bool, vertical: bool) -> Self {
        Self {
            portal: portal.to_pod(),
            horizontal,
            vertical,
            contact: None,
            suppressed_button: None,
            generation: 0,
        }
    }

    pub(super) fn portal_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, Portal<W>> {
        this.ctx.get_mut(&mut this.widget.portal)
    }

    fn move_touch(&mut self, ctx: &mut EventCtx<'_>, pointer: Option<PointerId>, point: Point) {
        let Some(contact) = &mut self.contact else {
            return;
        };
        if contact.pointer != pointer || !point.is_finite() {
            self.contact = None;
            return;
        }
        if !contact.allowed {
            return;
        }
        let distance = point - contact.start;
        let distance = if self.horizontal {
            distance.y.abs()
        } else if self.vertical {
            distance.x.abs()
        } else {
            distance.hypot()
        };
        if !contact.dragging && distance < DRAG_THRESHOLD {
            return;
        }
        contact.dragging = true;
        let mut delta = contact.previous - point;
        contact.previous = point;
        if self.horizontal {
            delta.x = 0.0;
        }
        if self.vertical {
            delta.y = 0.0;
        }
        ctx.mutate_child_later(&mut self.portal, move |mut portal| {
            Portal::pan_viewport_by(&mut portal, delta);
        });
        ctx.set_handled();
    }
}

impl<W: Widget + FromDynWidget + ?Sized> Widget for TouchScroll<W> {
    type Action = NoAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(event) if event.pointer.pointer_type == PointerType::Touch => {
                self.suppressed_button = None;
                self.generation = self.generation.wrapping_add(1);
                if self.contact.take().is_some() || ctx.is_disabled() {
                    return;
                }
                let point = ctx.local_position(event.state.position);
                if !point.is_finite() {
                    return;
                }
                let capture = ctx.pointer_capture_target_id();
                self.contact = Some(Contact {
                    pointer: event.pointer.pointer_id,
                    start: point,
                    previous: point,
                    button: capture,
                    allowed: capture.is_none(),
                    dragging: false,
                });
                if let Some(target) = capture {
                    // Button rows can scroll without stealing pointer capture from
                    // editors, sliders, scrollbars or custom direct controls.
                    let owner = ctx.widget_id();
                    let generation = self.generation;
                    ctx.mutate_later(target, move |mut target| {
                        let allowed = target.widget.accessibility_role() == Role::Button;
                        target.ctx.mutate_later(owner, move |mut owner| {
                            let owner = owner.downcast::<Self>();
                            if owner.widget.generation == generation {
                                if allowed {
                                    if let Some(contact) = &mut owner.widget.contact {
                                        contact.allowed = true;
                                    }
                                } else {
                                    owner.widget.contact = None;
                                }
                            }
                        });
                    });
                } else {
                    ctx.capture_pointer();
                }
            }
            PointerEvent::Move(event) if event.pointer.pointer_type == PointerType::Touch => {
                self.move_touch(
                    ctx,
                    event.pointer.pointer_id,
                    ctx.local_position(event.current.position),
                );
            }
            PointerEvent::Up(event) if event.pointer.pointer_type == PointerType::Touch => {
                self.move_touch(
                    ctx,
                    event.pointer.pointer_id,
                    ctx.local_position(event.state.position),
                );
                if let Some(contact) = self.contact.take() {
                    self.suppressed_button = contact.dragging.then_some(contact.button).flatten();
                    // This marker runs after any ButtonPress queued by the same
                    // Up. It clears the guard even when release missed the button,
                    // so a subsequent keyboard/accessibility click is unaffected.
                    ctx.mutate_child_later(&mut self.portal, |mut portal| {
                        portal.ctx.submit_untyped_action(Box::new(EndTouch));
                    });
                }
            }
            PointerEvent::Cancel(_) | PointerEvent::Down(_) => {
                self.contact = None;
                self.suppressed_button = None;
            }
            _ => {}
        }
    }

    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if action.is::<EndTouch>()
            || (action.is::<ButtonPress>() && self.suppressed_button == Some(source))
        {
            self.suppressed_button = None;
            ctx.set_handled();
        }
    }

    fn on_text_event(
        &mut self,
        _: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if matches!(event, TextEvent::WindowFocusChange(false)) {
            self.contact = None;
        }
    }

    fn update(&mut self, _: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(
            event,
            Update::DisabledChanged(true) | Update::StashedChanged(true)
        ) {
            self.contact = None;
            self.suppressed_button = None;
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.portal);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        cross: Option<Length>,
    ) -> Length {
        ctx.compute_length(
            &mut self.portal,
            request.into(),
            LayoutSize::maybe(axis.cross(), cross),
            axis,
            cross,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.portal, size);
        ctx.place_child(&mut self.portal, Point::ORIGIN);
        ctx.derive_baselines(&self.portal);
    }
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.portal.id()])
    }
}
