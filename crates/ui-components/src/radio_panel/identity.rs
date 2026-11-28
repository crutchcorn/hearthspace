//! Replace a radio row's widgets when its device/action identity changes, so
//! an old press or queued accessibility action cannot activate a replacement.
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::widgets::SizedBox,
};
pub(super) fn identified<V>(key: String, inner: V) -> Identified<V> {
    Identified { key, inner }
}
pub(super) struct Identified<V> {
    key: String,
    inner: V,
}
impl<V> ViewMarker for Identified<V> {}
impl<S: 'static, V: WidgetView<S>> View<S, (), ViewCtx> for Identified<V> {
    type Element = Pod<SizedBox>;
    type ViewState = (V::ViewState, u64);
    fn build(&self, ctx: &mut ViewCtx, s: &mut S) -> (Self::Element, Self::ViewState) {
        let (child, state) = ctx.with_id(ViewId::new(0), |ctx| self.inner.build(ctx, s));
        (ctx.create_pod(SizedBox::new(child.new_widget)), (state, 0))
    }
    fn rebuild(
        &self,
        old: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
        s: &mut S,
    ) {
        if self.key != old.key {
            ctx.with_id(ViewId::new(vs.1), |ctx| {
                old.inner.teardown(
                    &mut vs.0,
                    ctx,
                    SizedBox::child_mut(&mut el).unwrap().downcast(),
                )
            });
            vs.1 = vs.1.wrapping_add(1);
            let (child, state) = ctx.with_id(ViewId::new(vs.1), |ctx| self.inner.build(ctx, s));
            SizedBox::set_child(&mut el, child.new_widget);
            vs.0 = state;
        } else {
            ctx.with_id(ViewId::new(vs.1), |ctx| {
                self.inner.rebuild(
                    &old.inner,
                    &mut vs.0,
                    ctx,
                    SizedBox::child_mut(&mut el).unwrap().downcast(),
                    s,
                )
            });
        }
    }
    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(ViewId::new(vs.1), |ctx| {
            self.inner.teardown(
                &mut vs.0,
                ctx,
                SizedBox::child_mut(&mut el).unwrap().downcast(),
            )
        });
    }
    fn message(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut MessageCtx,
        mut el: Mut<'_, Self::Element>,
        s: &mut S,
    ) -> MessageResult<()> {
        if ctx.take_first() != Some(ViewId::new(vs.1)) {
            return MessageResult::Stale;
        }
        self.inner.message(
            &mut vs.0,
            ctx,
            SizedBox::child_mut(&mut el).unwrap().downcast(),
            s,
        )
    }
}
