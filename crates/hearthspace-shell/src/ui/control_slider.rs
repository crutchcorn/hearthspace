//! A named accessible view of Masonry's slider; interaction stays upstream.
use super::{control_state::Action, state::ShellState};
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
        PointerEvent, PrePaintProps, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
        UpdateCtx, Widget, paint_background, paint_box_shadow, pointer::PointerButton,
    },
    imaging::Painter,
    kurbo::{Axis, Size, Stroke},
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
                    keyboard_focus: false,
                    pointer_focus_pending: false,
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
                        color: hearthspace_ui_components::TEXT,
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
    // Masonry currently exposes focus, but not input modality. Keep this outside
    // `inner`, which the view replaces when the backend reports a new value.
    keyboard_focus: bool,
    pointer_focus_pending: bool,
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
        if let PointerEvent::Down(down) = event
            && !ctx.is_disabled()
        {
            self.keyboard_focus = false;
            self.pointer_focus_pending = matches!(down.button, Some(PointerButton::Primary) | None)
                && !ctx.is_focus_target();
            ctx.request_paint_only();
        }
        self.inner.on_pointer_event(ctx, props, event);
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Keyboard(key) = event
            && key.state.is_down()
            && ctx.is_focus_target()
            && !ctx.is_disabled()
        {
            self.keyboard_focus = true;
            self.pointer_focus_pending = false;
            ctx.request_paint_only();
        }
        if matches!(event, TextEvent::WindowFocusChange(_)) {
            ctx.request_paint_only();
        }
        self.inner.on_text_event(ctx, props, event);
    }
    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == xilem::masonry::accesskit::Action::Focus && !ctx.is_disabled() {
            self.keyboard_focus = true;
            self.pointer_focus_pending = false;
            ctx.request_paint_only();
        }
        self.inner.on_access_event(ctx, props, event);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::FocusChanged(focused) => {
                self.keyboard_focus = *focused && !self.pointer_focus_pending;
                self.pointer_focus_pending = false;
            }
            Update::DisabledChanged(true) | Update::StashedChanged(true) => {
                self.keyboard_focus = false;
                self.pointer_focus_pending = false;
                ctx.request_paint_only();
            }
            _ => {}
        }
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
        if self.keyboard_focus
            && ctx.is_focus_target()
            && ctx.is_window_focused()
            && !ctx.is_disabled()
        {
            // Upstream also outlines hovered/pointer-focused sliders, and its
            // positive inset expands outside the clipped popup. Keep our stroke
            // fully inside the widget (kurbo's negative inset shrinks), painting
            // after the track so it cannot cover the ring's left/right edges.
            let color = props.get::<BorderColor>(ctx.property_cache()).color;
            painter
                .stroke(
                    ctx.border_box().inset(-2.0).to_rounded_rect(4.0),
                    &Stroke::new(2.0),
                    color,
                )
                .draw();
        }
    }
    fn pre_paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let bbox = ctx.border_box();
        let p = PrePaintProps::fetch(props, ctx.property_cache());
        paint_box_shadow(painter, bbox, p.box_shadow, p.corner_radius);
        paint_background(painter, bbox, p.background, p.border_width, p.corner_radius);
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

#[cfg(test)]
#[path = "control_slider_tests.rs"]
mod tests;
