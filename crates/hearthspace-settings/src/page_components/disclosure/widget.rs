use hearthspace_ui_components as components;
use xilem::masonry::{
    accesskit::{Action, Node, Role},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
        PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent,
        Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
        keyboard::{Key, NamedKey},
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size, Stroke},
    layout::{LayoutSize, LenReq, Length},
    widgets::ButtonPress,
};

pub(crate) struct DisclosureButton {
    pub(super) name: String,
    pub(super) expanded: bool,
    child: WidgetPod<dyn Widget>,
    focus: components::settings_material::FocusIndicator,
}

impl DisclosureButton {
    pub(super) fn new(
        name: String,
        expanded: bool,
        child: NewWidget<impl Widget + ?Sized>,
    ) -> Self {
        Self {
            name,
            expanded,
            child: child.erased().to_pod(),
            focus: components::settings_material::FocusIndicator::default(),
        }
    }

    pub(super) fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for DisclosureButton {
    type Action = ButtonPress;

    // Match native Button activation, including release and pointer capture.
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.focus.pointer(event);
        match event {
            PointerEvent::Down(..) => {
                ctx.request_focus();
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Up(PointerButtonEvent { button, .. }) => {
                if ctx.is_active() && ctx.is_hovered() {
                    ctx.submit_action::<Self::Action>(ButtonPress { button: *button });
                }
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        self.focus.text(event);
        ctx.request_paint_only();
        if let TextEvent::Keyboard(event) = event
            && event.state.is_up()
            && (matches!(&event.key, Key::Character(c) if c == " ")
                || event.key == Key::Named(NamedKey::Enter))
        {
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == Action::Click {
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        self.focus.update(event);
        if matches!(
            event,
            Update::FocusChanged(_) | Update::HoveredChanged(_) | Update::ActiveChanged(_)
        ) {
            ctx.request_paint_only();
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        cross: Option<Length>,
    ) -> Length {
        ctx.compute_length(
            &mut self.child,
            request.into(),
            LayoutSize::maybe(axis.cross(), cross),
            axis,
            cross,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
        ctx.set_paint_insets(24.0);
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        components::settings_material::paint_button(
            painter,
            ctx.border_box().inset(-1.0),
            components::RADIUS_CONTROL,
            components::SettingsButton::Bare,
            false,
            components::settings_material::Interaction {
                hovered: ctx.is_hovered(),
                active: ctx.is_active(),
                focused: false,
                disabled: ctx.is_disabled(),
            },
        );
        if ctx.is_focus_target() && self.focus.visible() {
            painter
                .stroke(
                    ctx.border_box()
                        .inset(-3.0)
                        .to_rounded_rect(components::RADIUS_CONTROL),
                    &Stroke::new(2.0),
                    components::FOCUS_RING,
                )
                .draw();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.name.clone());
        node.set_expanded(self.expanded);
        node.add_action(Action::Click);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }
}
