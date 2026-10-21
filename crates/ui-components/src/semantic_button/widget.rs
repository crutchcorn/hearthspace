use xilem::masonry::{
    accesskit::{Node, Role, Toggled},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
        PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Widget, WidgetMut,
        WidgetPod,
    },
    imaging::Painter,
    kurbo::{Axis, Size},
    layout::{LayoutSize, LenReq, Length, SizeDef},
    widgets::{Button, ButtonPress, SizedBox},
};

/// The input behavior is Masonry's Button. Presentation and semantics are owned
/// here because upstream Button has no name or toggle-state builder yet.
/// Its unused child is never registered: our real content is the only subtree.
pub struct SemanticButton {
    behavior: Button,
    child: WidgetPod<dyn Widget>,
    pub(super) name: String,
    pub(super) pressed: Option<bool>,
}
impl SemanticButton {
    pub(super) fn new(
        child: NewWidget<impl Widget + ?Sized>,
        name: &str,
        pressed: Option<bool>,
    ) -> Self {
        Self {
            behavior: Button::new(NewWidget::new(SizedBox::empty())),
            child: child.erased().to_pod(),
            name: name.into(),
            pressed,
        }
    }
    pub(super) fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}
impl Widget for SemanticButton {
    type Action = ButtonPress;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.behavior.on_pointer_event(ctx, props, event);
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        self.behavior.on_text_event(ctx, props, event);
    }
    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        self.behavior.on_access_event(ctx, props, event);
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        cross: Option<Length>,
    ) -> Length {
        let length = ctx.compute_length(
            &mut self.child,
            request.into(),
            LayoutSize::maybe(axis.cross(), cross),
            axis,
            cross,
        );
        if axis == Axis::Vertical {
            length.max(xilem::masonry::theme::BASIC_WIDGET_HEIGHT)
        } else {
            length
        }
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        let child = ctx.compute_size(&mut self.child, SizeDef::fit(size), size.into());
        ctx.run_layout(&mut self.child, child);
        ctx.place_child(&mut self.child, ((size - child).to_vec2() * 0.5).to_point());
        ctx.derive_baselines(&self.child);
    }
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        Role::Button
    }
    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        self.behavior.accessibility(ctx, props, node);
        node.set_label(self.name.clone());
        if let Some(pressed) = self.pressed {
            node.set_toggled(if pressed {
                Toggled::True
            } else {
                Toggled::False
            });
        }
    }
    fn accepts_focus(&self) -> bool {
        self.behavior.accepts_focus()
    }
    fn accepts_pointer_interaction(&self) -> bool {
        self.behavior.accepts_pointer_interaction()
    }
    fn propagates_pointer_interaction(&self) -> bool {
        false
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xilem::masonry::{
        core::{
            PointerButton,
            keyboard::{Key, NamedKey},
        },
        testing::TestHarness,
        widgets::Label,
    };
    #[test]
    fn named_pressed_button_keeps_native_keyboard_pointer_and_access_activation() {
        let widget = SemanticButton::new(
            NewWidget::new(Label::new("14:52")),
            "Date and time",
            Some(true),
        );
        let mut harness =
            TestHarness::create(crate::hearthspace_light_properties(), widget.prepare());
        let id = harness.root_id();
        let node = harness.access_node(id).unwrap();
        assert_eq!(node.label().as_deref(), Some("Date and time"));
        assert_eq!(node.toggled(), Some(Toggled::True));
        harness.press_tab_key(false);
        assert_eq!(harness.focused_widget_id(), Some(id));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
        harness.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert_eq!(
            harness.pop_action::<ButtonPress>(),
            Some((ButtonPress { button: None }, id))
        );
        harness.mouse_click_on(id, Some(PointerButton::Primary));
        assert_eq!(
            harness.pop_action::<ButtonPress>(),
            Some((
                ButtonPress {
                    button: Some(PointerButton::Primary)
                },
                id
            ))
        );
    }
}
