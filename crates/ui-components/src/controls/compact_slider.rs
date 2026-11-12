//! Compact percentage slider with configurable minimum and one-percent steps.
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
        PointerEvent, PrePaintProps, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
        UpdateCtx, Widget, paint_background, paint_box_shadow, pointer::PointerButton,
    },
    imaging::Painter,
    kurbo::{Axis, Size, Stroke},
    layout::{LenReq, Length},
    properties::BorderColor,
    widgets::{Slider, SliderMoved},
};
pub struct NamedSlider {
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

impl NamedSlider {
    pub fn new(name: &'static str, minimum: f64, value: f64) -> Self {
        Self {
            name,
            inner: Slider::new(minimum, 100.0, value).with_step(1.0),
            keyboard_focus: false,
            pointer_focus_pending: false,
        }
    }
    /// Replace the backend snapshot without resetting keyboard focus modality.
    pub fn set_level(&mut self, minimum: f64, value: f64) {
        self.inner = Slider::new(minimum, 100.0, value).with_step(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xilem::masonry::{
        core::{
            NewWidget,
            keyboard::{Key, NamedKey},
        },
        testing::TestHarness,
    };
    #[test]
    fn nonzero_minimum_survives_keyboard_and_accessible_value_updates() {
        let mut h = TestHarness::create_with_size(
            crate::hearthspace_opaque_properties(),
            NewWidget::new(NamedSlider::new("Brightness", 5., 40.)),
            (220, 32),
        );
        let id = h.root_id();
        h.press_tab_key(false);
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Home)));
        assert_eq!(
            h.pop_action::<SliderMoved>(),
            Some((SliderMoved { value: 5. }, id))
        );
        h.edit_root_widget(|mut w| {
            w.widget.set_level(5., 70.);
            w.ctx.request_render();
            w.ctx.request_accessibility_update();
        });
        h.redraw();
        let n = h.access_node(id).unwrap();
        assert_eq!(n.label().as_deref(), Some("Brightness"));
        assert_eq!(n.min_numeric_value(), Some(5.));
        assert_eq!(n.numeric_value(), Some(70.));
        h.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowRight)));
        assert_eq!(
            h.pop_action::<SliderMoved>(),
            Some((SliderMoved { value: 71. }, id))
        );
    }
}
