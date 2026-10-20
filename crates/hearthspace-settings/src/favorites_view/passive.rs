//! Drawing prompts paint and describe the pad, but never become pointer targets.
use crate::state::SettingsApp;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef,
            RegisterCtx, Widget, WidgetMut, WidgetPod,
        },
        imaging::Painter,
        kurbo::{Axis, Point, Size},
        layout::{LenReq, Length},
    },
};

pub(super) fn passive<V: WidgetView<SettingsApp>>(inner: V) -> Passive<V> {
    Passive(inner)
}
pub(super) struct Passive<V>(V);
impl<V> ViewMarker for Passive<V> {}
const CHILD: ViewId = ViewId::new(0);
impl<V: WidgetView<SettingsApp>> View<SettingsApp, (), ViewCtx> for Passive<V> {
    type Element = Pod<PassiveWidget>;
    type ViewState = V::ViewState;
    fn build(
        &self,
        ctx: &mut ViewCtx,
        state: &mut SettingsApp,
    ) -> (Self::Element, Self::ViewState) {
        let (child, view_state) = ctx.with_id(CHILD, |ctx| self.0.build(ctx, state));
        (
            ctx.create_pod(PassiveWidget {
                child: child.new_widget.erased().to_pod(),
            }),
            view_state,
        )
    }
    fn rebuild(
        &self,
        old: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) {
        ctx.with_id(CHILD, |ctx| {
            self.0.rebuild(
                &old.0,
                view_state,
                ctx,
                PassiveWidget::child(&mut element).downcast(),
                state,
            )
        });
    }
    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(CHILD, |ctx| {
            self.0.teardown(
                view_state,
                ctx,
                PassiveWidget::child(&mut element).downcast(),
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
        if message.take_first() == Some(CHILD) {
            self.0.message(
                view_state,
                message,
                PassiveWidget::child(&mut element).downcast(),
                state,
            )
        } else {
            MessageResult::Stale
        }
    }
}

pub(super) struct PassiveWidget {
    child: WidgetPod<dyn Widget>,
}
impl PassiveWidget {
    fn child<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}
impl Widget for PassiveWidget {
    type Action = NoAction;
    fn accepts_pointer_interaction(&self) -> bool {
        false
    }
    fn propagates_pointer_interaction(&self) -> bool {
        false
    }
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
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
