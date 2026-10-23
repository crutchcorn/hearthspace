//! Observes descendant hover/focus without replacing the button's native input.
use super::ShellState;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
            PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx,
            Widget, WidgetMut, WidgetPod,
            keyboard::{Key, NamedKey},
        },
        imaging::Painter,
        kurbo::{Axis, Point, Size},
        layout::{LenReq, Length},
    },
};

pub(in crate::ui::frame_layout) fn hint<V: WidgetView<ShellState>>(
    inner: V,
    label: &'static str,
) -> Hint<V> {
    Hint { inner, label }
}
pub(in crate::ui::frame_layout) struct Hint<V> {
    inner: V,
    label: &'static str,
}
impl<V> ViewMarker for Hint<V> {}
const CHILD: ViewId = ViewId::new(0);
impl<V: WidgetView<ShellState>> View<ShellState, (), ViewCtx> for Hint<V> {
    type Element = Pod<HintWidget>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let (child, vs) = ctx.with_id(CHILD, |ctx| self.inner.build(ctx, state));
        let element =
            ctx.with_action_widget(|ctx| ctx.create_pod(HintWidget::new(child.new_widget)));
        (element, vs)
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
                HintWidget::child_mut(&mut el).downcast(),
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
        ctx.with_id(CHILD, |ctx| {
            self.inner
                .teardown(vs, ctx, HintWidget::child_mut(&mut el).downcast())
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
            Some(CHILD) => {
                let result =
                    self.inner
                        .message(vs, ctx, HintWidget::child_mut(&mut el).downcast(), state);
                if matches!(result, MessageResult::Action(())) {
                    el.widget.dismissed = true;
                    super::change(state, self.label, false);
                }
                result
            }
            None => {
                if let Some(visible) = ctx.take_message::<Visible>() {
                    super::change(state, self.label, visible.0);
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
pub(in crate::ui::frame_layout) struct Visible(bool);
pub(in crate::ui::frame_layout) struct HintWidget {
    child: WidgetPod<dyn Widget>,
    active: bool,
    hovered: bool,
    focused: bool,
    dismissed: bool,
}
impl HintWidget {
    fn new(child: NewWidget<impl Widget + ?Sized>) -> Self {
        Self {
            child: child.erased().to_pod(),
            active: false,
            hovered: false,
            focused: false,
            dismissed: false,
        }
    }
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
    fn dismiss(&mut self, ctx: &mut EventCtx<'_>) {
        self.dismissed = true;
        ctx.submit_action::<Visible>(Visible(false));
    }
}
impl Widget for HintWidget {
    type Action = Visible;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        let changed = match event {
            Update::ChildHoveredChanged(value) => {
                self.hovered = *value;
                true
            }
            Update::ChildFocusChanged(value) => {
                self.focused = *value;
                true
            }
            _ => false,
        };
        if changed {
            let active = self.hovered || self.focused;
            if !active {
                self.dismissed = false;
            }
            if self.active != active {
                self.active = active;
                let visible = active && !self.dismissed;
                ctx.mutate_self_later(move |mut this| {
                    this.ctx.submit_action::<Visible>(Visible(visible))
                });
            }
        }
    }
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if matches!(event, PointerEvent::Down(_)) {
            self.dismiss(ctx);
        }
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Keyboard(key) = event
            && key.state.is_down()
            && (key.key == Key::Named(NamedKey::Enter) || key.key == Key::Character(" ".into()))
        {
            self.dismiss(ctx);
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
