//! A named accessible view of Masonry's slider; interaction stays upstream.
use super::{control_state::Action, state::ShellState};
pub(super) use hearthspace_ui_components::controls::compact_slider::NamedSlider;
#[cfg(test)]
use xilem::masonry::{
    core::{TextEvent, pointer::PointerButton},
    properties::BorderColor,
};
use xilem::masonry::{
    layout::AsUnit,
    properties::{ThumbRadius, TrackThickness},
    widgets::SliderMoved,
};
use xilem::{
    Pod, ViewCtx,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
};

pub(super) struct ControlSlider {
    pub name: &'static str,
    pub output: String,
    pub minimum: u8,
    pub value: u8,
    pub enabled: bool,
}

impl ViewMarker for ControlSlider {}
impl View<ShellState, (), ViewCtx> for ControlSlider {
    type Element = Pod<NamedSlider>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut ShellState) -> (Self::Element, ()) {
        (
            ctx.with_action_widget(|ctx| {
                let mut pod = ctx.create_pod(NamedSlider::new(
                    self.name,
                    f64::from(self.minimum),
                    f64::from(self.value),
                ));
                pod.new_widget = pod
                    .new_widget
                    .with_props((TrackThickness(4.0.px()), ThumbRadius(7.0.px())));
                pod.new_widget.options.disabled = !self.enabled;
                pod
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        old: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut ShellState,
    ) {
        if self.enabled != old.enabled {
            element.ctx.set_disabled(!self.enabled);
        }
        if self.value != old.value || self.minimum != old.minimum {
            element
                .widget
                .set_level(f64::from(self.minimum), f64::from(self.value));
            element.ctx.request_render();
            element.ctx.request_accessibility_update();
        }
    }

    fn teardown(&self, _: &mut (), ctx: &mut ViewCtx, element: Mut<'_, Self::Element>) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        _: &mut (),
        message: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        if message.take_first().is_some() {
            return MessageResult::Stale;
        }
        if let Some(moved) = message.take_message::<SliderMoved>() {
            if self.enabled && moved.value.is_finite() {
                let value = moved.value.round().clamp(f64::from(self.minimum), 100.0) as u8;
                let action = if self.name == "Brightness" {
                    Action::SetBrightness(value)
                } else {
                    Action::SetVolume(value)
                };
                super::control_task::submit(state, &self.output, action, None);
            }
            MessageResult::Action(())
        } else {
            MessageResult::Stale
        }
    }
}

#[cfg(test)]
#[path = "control_slider_tests.rs"]
mod tests;
