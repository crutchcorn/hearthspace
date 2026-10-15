//! A named accessible view of Masonry's slider; interaction stays upstream.
use super::{control_state::Action, state::ShellState};
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
        PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx,
        Widget,
    },
    imaging::Painter,
    kurbo::{Axis, Size},
    layout::{AsUnit, LenReq, Length},
    properties::{BorderColor, ThumbColor, ThumbRadius, TrackColor, TrackThickness},
    widgets::{Slider, SliderMoved},
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

impl ControlSlider {
    fn slider(&self) -> Slider {
        Slider::new(f64::from(self.minimum), 100.0, f64::from(self.value)).with_step(1.0)
    }
}

impl ViewMarker for ControlSlider {}
impl View<ShellState, (), ViewCtx> for ControlSlider {
    type Element = Pod<NamedSlider>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut ShellState) -> (Self::Element, ()) {
        (
            ctx.with_action_widget(|ctx| {
                let mut pod = ctx.create_pod(NamedSlider {
                    name: self.name,
                    inner: self.slider(),
                });
                pod.new_widget = pod.new_widget.with_props((
                    TrackThickness(4.0.px()),
                    ThumbRadius(7.0.px()),
                    TrackColor {
                        active: hearthspace_ui_components::ACCENT,
                        inactive: hearthspace_ui_components::BORDER,
                    },
                    ThumbColor(hearthspace_ui_components::SURFACE),
                    BorderColor {
                        color: hearthspace_ui_components::ACCENT,
                    },
                ));
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
            element.widget.inner = self.slider();
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

pub(super) struct NamedSlider {
    name: &'static str,
    inner: Slider,
}

impl Widget for NamedSlider {
    type Action = SliderMoved;
    fn accepts_focus(&self) -> bool {
        self.inner.accepts_focus()
    }
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        self.inner.register_children(ctx);
    }
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.inner.on_pointer_event(ctx, props, event);
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        self.inner.on_text_event(ctx, props, event);
    }
    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        self.inner.on_access_event(ctx, props, event);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, props: &mut PropertiesMut<'_>, event: &Update) {
        self.inner.update(ctx, props, event);
    }
    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property: std::any::TypeId) {
        self.inner.property_changed(ctx, property);
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        props: &PropertiesRef<'_>,
        axis: Axis,
        len: LenReq,
        cross: Option<Length>,
    ) -> Length {
        self.inner.measure(ctx, props, axis, len, cross)
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, props: &PropertiesRef<'_>, size: Size) {
        self.inner.layout(ctx, props, size);
    }
    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        self.inner.paint(ctx, props, painter);
    }
    fn pre_paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        self.inner.pre_paint(ctx, props, painter);
    }
    fn accessibility_role(&self) -> Role {
        self.inner.accessibility_role()
    }
    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        self.inner.accessibility(ctx, props, node);
        node.set_label(self.name);
    }
    fn children_ids(&self) -> ChildrenIds {
        self.inner.children_ids()
    }
}
