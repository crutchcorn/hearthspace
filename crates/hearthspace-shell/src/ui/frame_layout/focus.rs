//! Ordered native control identities bridge Tab between independent surfaces.
use super::{FocusRequest, ShellState};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ActionCtx, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx,
            NoAction, PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Widget,
            WidgetId, WidgetMut, WidgetPod,
            keyboard::{Key, NamedKey},
        },
        imaging::Painter,
        kurbo::{Axis, Point, Size},
        layout::{LenReq, Length},
    },
};

#[derive(Clone, Default)]
pub(in crate::ui) struct Chain(Arc<Mutex<BTreeMap<usize, WidgetId>>>);
impl Chain {
    fn end(&self, reverse: bool) -> Option<WidgetId> {
        let ids = self.0.lock().unwrap();
        if reverse {
            ids.values().next_back().copied()
        } else {
            ids.values().next().copied()
        }
    }
}

pub(in crate::ui) fn item<V: WidgetView<ShellState>>(
    inner: V,
    chain: &Chain,
    index: usize,
) -> Item<V> {
    Item {
        inner,
        chain: chain.clone(),
        index,
    }
}
pub(in crate::ui) struct Item<V> {
    inner: V,
    chain: Chain,
    index: usize,
}
impl<V> ViewMarker for Item<V> {}
impl<V: WidgetView<ShellState>> View<ShellState, (), ViewCtx> for Item<V> {
    type Element = Pod<V::Widget>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let result = self.inner.build(ctx, state);
        self.chain
            .0
            .lock()
            .unwrap()
            .insert(self.index, result.0.new_widget.id());
        result
    }
    fn rebuild(
        &self,
        prev: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) {
        let id = element.id();
        if self.index != prev.index {
            prev.chain.0.lock().unwrap().remove(&prev.index);
        }
        self.chain.0.lock().unwrap().insert(self.index, id);
        self.inner.rebuild(&prev.inner, vs, ctx, element, state);
    }
    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        self.chain.0.lock().unwrap().remove(&self.index);
        self.inner.teardown(vs, ctx, element);
    }
    fn message(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        self.inner.message(vs, ctx, element, state)
    }
}

pub(super) fn boundary<V: WidgetView<ShellState>>(
    inner: V,
    chain: Chain,
    output: String,
    target: hearthspace_common::frame_layout::LayoutTarget,
    request: Option<FocusRequest>,
) -> Boundary<V> {
    Boundary {
        inner,
        chain,
        output,
        target,
        request,
    }
}
pub(super) struct Boundary<V> {
    inner: V,
    chain: Chain,
    output: String,
    target: hearthspace_common::frame_layout::LayoutTarget,
    request: Option<FocusRequest>,
}
impl<V> ViewMarker for Boundary<V> {}
const CHILD: ViewId = ViewId::new(0);
impl<V: WidgetView<ShellState>> View<ShellState, (), ViewCtx> for Boundary<V> {
    type Element = Pod<BoundaryWidget>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let (inner, vs) = ctx.with_id(CHILD, |ctx| self.inner.build(ctx, state));
        let pod = ctx.with_action_widget(|ctx| {
            ctx.create_pod(BoundaryWidget {
                inner: inner.new_widget.erased().to_pod(),
                chain: self.chain.clone(),
            })
        });
        (pod, vs)
    }
    fn rebuild(
        &self,
        prev: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) {
        ctx.with_id(CHILD, |ctx| {
            self.inner.rebuild(
                &prev.inner,
                vs,
                ctx,
                BoundaryWidget::child_mut(&mut el).downcast(),
                state,
            )
        });
        if self.request != prev.request
            && let Some(request) = &self.request
        {
            let reverse = request.reverse;
            let WidgetMut { widget, ctx, .. } = &mut el;
            ctx.mutate_child_later(&mut widget.inner, move |mut child| {
                child.ctx.submit_untyped_action(Box::new(Enter(reverse)))
            });
        }
    }
    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(CHILD, |ctx| {
            self.inner
                .teardown(vs, ctx, BoundaryWidget::child_mut(&mut el).downcast())
        });
        ctx.teardown_action_source(el);
    }
    fn message(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut MessageCtx,
        mut el: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        match ctx.take_first() {
            Some(CHILD) => self.inner.message(
                vs,
                ctx,
                BoundaryWidget::child_mut(&mut el).downcast(),
                state,
            ),
            None => {
                if let Some(action) = ctx.take_message::<Leave>() {
                    let reverse = action.0;
                    super::submit(
                        state,
                        format!(
                            "frame-focus-next {}",
                            serde_json::json!({"output":self.output,"target":self.target,"reverse":reverse})
                        ),
                    );
                    MessageResult::Action(())
                } else {
                    MessageResult::Stale
                }
            }
            _ => MessageResult::Stale,
        }
    }
}
#[derive(Debug)]
struct Leave(bool);
#[derive(Debug)]
struct Enter(bool);
pub(super) struct BoundaryWidget {
    inner: WidgetPod<dyn Widget>,
    chain: Chain,
}
impl BoundaryWidget {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.inner)
    }
}
impl Widget for BoundaryWidget {
    type Action = NoAction;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.inner);
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        match event {
            TextEvent::WindowFocusChange(true) if !ctx.has_focus_target() => {
                if let Some(id) = self.chain.end(false) {
                    ctx.set_focus(id);
                }
            }
            TextEvent::WindowFocusChange(false) if ctx.has_focus_target() => ctx.resign_focus(),
            TextEvent::Keyboard(key)
                if key.state.is_down()
                    && key.key == Key::Named(NamedKey::Tab)
                    && !key.modifiers.ctrl()
                    && !key.modifiers.alt()
                    && !key.modifiers.meta() =>
            {
                let reverse = key.modifiers.shift();
                if ctx.focus_target_id() == self.chain.end(!reverse) {
                    ctx.submit_untyped_action(Box::new(Leave(reverse)));
                    ctx.set_handled();
                }
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
        if source == self.inner.id()
            && let Some(Enter(reverse)) = action.downcast_ref::<Enter>()
        {
            if let Some(id) = self.chain.end(*reverse) {
                ctx.set_focus(id);
            }
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
    }
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.inner.id()])
    }
    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("FrameFocus", id = id.trace())
    }
}
