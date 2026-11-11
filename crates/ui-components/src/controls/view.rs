use super::{ControlKind, ControlWidget};
use xilem::{
    AnyWidgetView, Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::widgets::{SliderMoved, SwitchToggled},
};

/// Values are percentages for levels, and zero/one for boolean controls.
pub fn control<State: 'static, F: Fn(&mut State, f64) + Send + Sync + 'static>(
    kind: ControlKind,
    name: impl Into<String>,
    value: f64,
    disabled: bool,
    callback: F,
) -> Box<AnyWidgetView<State>> {
    let name = name.into();
    if kind == ControlKind::Slider {
        ControlView::<F, SliderMoved> {
            kind,
            name,
            value: normalized(value),
            disabled,
            callback,
            action: std::marker::PhantomData,
        }
        .boxed()
    } else {
        ControlView::<F, SwitchToggled> {
            kind,
            name,
            value: normalized(value),
            disabled,
            callback,
            action: std::marker::PhantomData,
        }
        .boxed()
    }
}
pub(super) fn normalized(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0., 100.)
    } else {
        0.
    }
}
struct ControlView<F, A = SwitchToggled> {
    action: std::marker::PhantomData<A>,
    kind: ControlKind,
    name: String,
    value: f64,
    disabled: bool,
    callback: F,
}
impl<F, A> ViewMarker for ControlView<F, A> {}
impl<
    State: 'static,
    A: std::fmt::Debug + Send + Sync + 'static,
    F: Fn(&mut State, f64) + Send + Sync + 'static,
> View<State, (), ViewCtx> for ControlView<F, A>
{
    type Element = Pod<ControlWidget<A>>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, ()) {
        (
            ctx.with_action_widget(|ctx| {
                let mut p =
                    ctx.create_pod(ControlWidget::<A>::new(self.kind, &self.name, self.value));
                p.new_widget.options.disabled = self.disabled;
                p
            }),
            (),
        )
    }
    fn rebuild(
        &self,
        old: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if self.value != old.value || self.kind != old.kind || self.name != old.name {
            el.widget.set_value(self.kind, &self.name, self.value);
            if self.kind != old.kind {
                el.ctx.request_layout();
            }
            el.ctx.request_render();
            el.ctx.request_accessibility_update();
        }
        if self.disabled != old.disabled {
            el.ctx.set_disabled(self.disabled);
        }
    }
    fn teardown(&self, _: &mut (), ctx: &mut ViewCtx, el: Mut<'_, Self::Element>) {
        ctx.teardown_action_source(el);
    }
    fn message(
        &self,
        _: &mut (),
        m: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<()> {
        let value = if let Some(v) = m.take_message::<SwitchToggled>() {
            Some(f64::from(v.0))
        } else {
            m.take_message::<SliderMoved>().map(|v| v.value)
        };
        if let Some(v) = value {
            if !self.disabled {
                (self.callback)(state, normalized(v));
            }
            MessageResult::Action(())
        } else {
            MessageResult::Stale
        }
    }
}
