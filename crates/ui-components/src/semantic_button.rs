//! Native button input with an explicit accessible name and optional pressed state.
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{core::PointerButton, widgets::ButtonPress},
};
mod widget;
pub use widget::SemanticButton;

pub fn semantic_button<
    State: 'static,
    V: WidgetView<State>,
    F: Fn(&mut State) + Send + Sync + 'static,
>(
    content: V,
    name: impl Into<String>,
    pressed: Option<bool>,
    action: F,
) -> SemanticButtonView<V, F> {
    SemanticButtonView {
        content,
        name: name.into(),
        pressed,
        radio: false,
        action,
    }
}

/// A radio choice whose entire arbitrary content shares one native focus stop.
/// The caller owns the selected value, as with Xilem's standard radio button.
pub fn semantic_radio_button<
    State: 'static,
    V: WidgetView<State>,
    F: Fn(&mut State) + Send + Sync + 'static,
>(
    content: V,
    name: impl Into<String>,
    checked: bool,
    action: F,
) -> SemanticButtonView<V, F> {
    SemanticButtonView {
        content,
        name: name.into(),
        pressed: Some(checked),
        radio: true,
        action,
    }
}

pub struct SemanticButtonView<V, F> {
    content: V,
    name: String,
    pressed: Option<bool>,
    radio: bool,
    action: F,
}
impl<V, F> ViewMarker for SemanticButtonView<V, F> {}
const CONTENT: ViewId = ViewId::new(0);
impl<State: 'static, V: WidgetView<State>, F: Fn(&mut State) + Send + Sync + 'static>
    View<State, (), ViewCtx> for SemanticButtonView<V, F>
{
    type Element = Pod<SemanticButton>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (content, vs) = ctx.with_id(CONTENT, |ctx| self.content.build(ctx, state));
        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(SemanticButton::new(
                    content.new_widget,
                    &self.name,
                    self.pressed,
                    self.radio,
                ))
            }),
            vs,
        )
    }
    fn rebuild(
        &self,
        prev: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        ctx.with_id(CONTENT, |ctx| {
            self.content.rebuild(
                &prev.content,
                vs,
                ctx,
                SemanticButton::child_mut(&mut el).downcast(),
                state,
            )
        });
        if self.name != prev.name || self.pressed != prev.pressed || self.radio != prev.radio {
            el.widget.name.clone_from(&self.name);
            el.widget.pressed = self.pressed;
            el.widget.radio = self.radio;
            el.ctx.request_accessibility_update();
        }
    }
    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(CONTENT, |ctx| {
            self.content
                .teardown(vs, ctx, SemanticButton::child_mut(&mut el).downcast())
        });
        ctx.teardown_action_source(el);
    }
    fn message(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut MessageCtx,
        mut el: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<()> {
        match ctx.take_first() {
            Some(CONTENT) => self.content.message(
                vs,
                ctx,
                SemanticButton::child_mut(&mut el).downcast(),
                state,
            ),
            None => {
                if let Some(press) = ctx.take_message::<ButtonPress>() {
                    if press.button.is_none() || press.button == Some(PointerButton::Primary) {
                        (self.action)(state);
                        MessageResult::Action(())
                    } else {
                        MessageResult::Nop
                    }
                } else {
                    MessageResult::Stale
                }
            }
            _ => MessageResult::Stale,
        }
    }
}
