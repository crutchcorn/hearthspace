//! Return to a persistent control after its transient choices disappear.
use crate::state::SettingsApp;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ActionCtx, ChildrenIds, ErasedAction, LayoutCtx, MeasureCtx, NoAction,
            PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx, Widget, WidgetId, WidgetMut,
            WidgetPod,
        },
        imaging::Painter,
        kurbo::{Axis, Point, Size},
        layout::{LenReq, Length},
    },
};

pub(crate) fn restore_focus_after_close<V: WidgetView<SettingsApp>>(
    inner: V,
    open: bool,
) -> RestoreFocus<V> {
    RestoreFocus { inner, open }
}

pub(crate) struct RestoreFocus<V> {
    inner: V,
    open: bool,
}
impl<V> ViewMarker for RestoreFocus<V> {}
const CONTENT: ViewId = ViewId::new(0);
impl<V: WidgetView<SettingsApp>> View<SettingsApp, (), ViewCtx> for RestoreFocus<V> {
    type Element = Pod<FocusScope>;
    type ViewState = V::ViewState;
    fn build(
        &self,
        ctx: &mut ViewCtx,
        state: &mut SettingsApp,
    ) -> (Self::Element, Self::ViewState) {
        let (inner, view_state) = ctx.with_id(CONTENT, |ctx| self.inner.build(ctx, state));
        (
            ctx.create_pod(FocusScope {
                inner: inner.new_widget.erased().to_pod(),
            }),
            view_state,
        )
    }
    fn rebuild(
        &self,
        previous: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) {
        ctx.with_id(CONTENT, |ctx| {
            self.inner.rebuild(
                &previous.inner,
                view_state,
                ctx,
                FocusScope::child_mut(&mut element).downcast(),
                state,
            )
        });
        if previous.open && !self.open {
            let WidgetMut { widget, ctx, .. } = &mut element;
            ctx.mutate_child_later(&mut widget.inner, return_to_control);
        }
    }
    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(CONTENT, |ctx| {
            self.inner.teardown(
                view_state,
                ctx,
                FocusScope::child_mut(&mut element).downcast(),
            )
        });
    }
    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) -> MessageResult<()> {
        match message.take_first() {
            Some(CONTENT) => self.inner.message(
                view_state,
                message,
                FocusScope::child_mut(&mut element).downcast(),
                state,
            ),
            _ => MessageResult::Stale,
        }
    }
}

#[derive(Debug)]
struct ReturnFocus;

fn return_to_control(mut child: WidgetMut<'_, dyn Widget>) {
    if child.widget.accepts_focus() {
        child.ctx.submit_untyped_action(Box::new(ReturnFocus));
    } else if let Some(&id) = child.widget.children_ids().first() {
        // Material and sizing decorators are not focus targets. This scope
        // contains one persistent control, reached through their first child.
        child.ctx.mutate_later(id, return_to_control);
    }
}

pub(crate) struct FocusScope {
    inner: WidgetPod<dyn Widget>,
}
impl FocusScope {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.inner)
    }
}
impl Widget for FocusScope {
    type Action = NoAction;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.inner);
    }
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if action.is::<ReturnFocus>() {
            ctx.set_focus(source);
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
        ctx.redirect_measurement(&mut self.inner, axis, cross)
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
