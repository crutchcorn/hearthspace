//! Record a native focus target and reveal it inside a scrollable popup.
use super::{FocusTarget, ShellState};
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ActionCtx, ChildrenIds, ErasedAction, LayoutCtx, MeasureCtx, NoAction,
            PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget,
            WidgetId, WidgetMut, WidgetPod,
        },
        imaging::Painter,
        kurbo::{Axis, Point, Size},
        layout::{LenReq, Length},
    },
};

pub(in crate::ui) struct Record<V> {
    pub(in crate::ui) inner: V,
    pub(in crate::ui) target: FocusTarget,
}
const CONTENT: ViewId = ViewId::new(0);
impl<V> ViewMarker for Record<V> {}
impl<V: WidgetView<ShellState>> View<ShellState, (), ViewCtx> for Record<V> {
    type Element = Pod<FocusRecord>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let (child, state) = ctx.with_id(CONTENT, |ctx| self.inner.build(ctx, state));
        *self.target.lock().unwrap() = Some(child.new_widget.id());
        (
            ctx.create_pod(FocusRecord {
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
        *self.target.lock().unwrap() = Some(el.widget.child.id());
        ctx.with_id(CONTENT, |ctx| {
            self.inner.rebuild(
                &prev.inner,
                vs,
                ctx,
                FocusRecord::child_mut(&mut el).downcast(),
                state,
            );
        });
    }
    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
    ) {
        *self.target.lock().unwrap() = None;
        ctx.with_id(CONTENT, |ctx| {
            self.inner
                .teardown(vs, ctx, FocusRecord::child_mut(&mut el).downcast());
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
            Some(CONTENT) => {
                self.inner
                    .message(vs, ctx, FocusRecord::child_mut(&mut el).downcast(), state)
            }
            _ => MessageResult::Stale,
        }
    }
}

pub(in crate::ui) struct FocusRecord {
    child: WidgetPod<dyn Widget>,
}
#[derive(Debug)]
struct RevealFocus;
impl FocusRecord {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}
impl Widget for FocusRecord {
    type Action = NoAction;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::ChildFocusChanged(true)) {
            ctx.mutate_child_later(&mut self.child, |mut child| {
                child.ctx.submit_untyped_action(Box::new(RevealFocus));
            });
        }
    }
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if source == self.child.id() && action.is::<RevealFocus>() {
            ctx.request_scroll_to_this();
            ctx.set_handled();
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
        ctx.derive_baselines(&self.child);
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
