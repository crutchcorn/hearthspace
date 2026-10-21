//! A focused descendant asks the compositor to reveal its exact local bounds.
use super::*;
use xilem::masonry::core::{Update, UpdateCtx};

pub(in crate::ui) struct Item<V> {
    pub(super) inner: V,
    pub(super) chain: Chain,
    pub(super) index: usize,
}
impl<V> ViewMarker for Item<V> {}
impl<V: WidgetView<ShellState>> View<ShellState, (), ViewCtx> for Item<V> {
    type Element = Pod<ItemWidget>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let (child, state) = ctx.with_id(CHILD, |ctx| self.inner.build(ctx, state));
        self.chain
            .0
            .lock()
            .unwrap()
            .insert(self.index, child.new_widget.id());
        (
            ctx.create_pod(ItemWidget {
                child: child.new_widget.erased().to_pod(),
            }),
            state,
        )
    }
    fn rebuild(
        &self,
        prev: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) {
        if self.index != prev.index {
            prev.chain.0.lock().unwrap().remove(&prev.index);
        }
        self.chain
            .0
            .lock()
            .unwrap()
            .insert(self.index, el.widget.child.id());
        ctx.with_id(CHILD, |ctx| {
            self.inner.rebuild(
                &prev.inner,
                vs,
                ctx,
                ItemWidget::child_mut(&mut el).downcast(),
                state,
            )
        });
    }
    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
    ) {
        self.chain.0.lock().unwrap().remove(&self.index);
        ctx.with_id(CHILD, |ctx| {
            self.inner
                .teardown(vs, ctx, ItemWidget::child_mut(&mut el).downcast())
        });
    }
    fn message(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut MessageCtx,
        mut el: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        match ctx.take_first() {
            Some(CHILD) => {
                self.inner
                    .message(vs, ctx, ItemWidget::child_mut(&mut el).downcast(), state)
            }
            _ => MessageResult::Stale,
        }
    }
}

pub(in crate::ui) struct ItemWidget {
    child: WidgetPod<dyn Widget>,
}
impl ItemWidget {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}
impl Widget for ItemWidget {
    type Action = NoAction;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::ChildFocusChanged(true)) {
            ctx.mutate_self_later(|mut widget| {
                let rect = widget.ctx.border_box();
                let origin = widget.ctx.to_window(rect.origin());
                widget.ctx.submit_untyped_action(Box::new(Reveal(
                    xilem::masonry::kurbo::Rect::from_origin_size(origin, rect.size()),
                )));
            });
        }
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        cross: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.child, axis, cross)
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
    }
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}
