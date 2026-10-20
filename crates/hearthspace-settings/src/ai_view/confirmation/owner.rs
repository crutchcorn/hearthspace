use super::{Decision, layer::ConfirmationLayer};
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ChildrenIds, EventCtx, LayerType, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
        PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx,
        Widget, WidgetId, WidgetMut, WidgetPod,
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size},
    layout::{LayoutSize, LenReq, Length},
};

pub(crate) struct ConfirmationOwner {
    inner: WidgetPod<dyn Widget>,
    pub open: bool,
    layer: Option<WidgetId>,
    keyboard: bool,
}

impl ConfirmationOwner {
    pub fn new(inner: NewWidget<impl Widget + ?Sized>, open: bool) -> Self {
        Self {
            inner: inner.erased().to_pod(),
            open,
            layer: None,
            keyboard: true,
        }
    }

    pub fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.inner)
    }

    pub fn sync_layer(this: &mut WidgetMut<'_, Self>) {
        if this.widget.open && this.widget.layer.is_none() {
            let layer = NewWidget::new(ConfirmationLayer::new(
                this.ctx.widget_id(),
                this.widget.keyboard,
            ));
            this.widget.layer = Some(layer.id());
            this.ctx
                .create_attached_layer(LayerType::Other, layer, Point::ORIGIN);
        } else if !this.widget.open
            && let Some(layer) = this.widget.layer.take()
        {
            this.ctx.remove_layer(layer);
        }
    }
}

impl Widget for ConfirmationOwner {
    type Action = Decision;
    fn on_pointer_event(
        &mut self,
        _: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if matches!(event, PointerEvent::Down(_)) {
            self.keyboard = false;
        }
    }
    fn on_text_event(
        &mut self,
        _: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if matches!(event, TextEvent::Keyboard(_)) {
            self.keyboard = true;
        }
    }
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.inner);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded) && self.open {
            ctx.mutate_self_later(|mut this| Self::sync_layer(&mut this.downcast()));
        }
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
            &mut self.inner,
            request.into(),
            LayoutSize::maybe(axis.cross(), cross),
            axis,
            cross,
        )
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.inner, size);
        ctx.place_child(&mut self.inner, Point::ORIGIN);
        ctx.derive_baselines(&self.inner);
    }
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.inner.id()])
    }
}
