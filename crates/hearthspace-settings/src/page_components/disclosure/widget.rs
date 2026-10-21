use hearthspace_ui_components as components;
use xilem::masonry::{
    accesskit::{Action, AriaCurrent, Node, Role, Toggled},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
        PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent,
        Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
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
    pub(super) selected: Option<bool>,
    pub(super) current_page: Option<bool>,
    pub(super) controls: Option<WidgetId>,
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
            selected: None,
            current_page: None,
            controls: None,
            child: child.erased().to_pod(),
            focus: components::settings_material::FocusIndicator::default(),
        }
    }

    pub(super) fn navigation_identity(&self) -> Option<super::NavigationControl> {
        (self.current_page.is_some() || self.controls.is_some()).then_some(
            super::NavigationControl {
                controls: self.controls,
                current_page: self.current_page,
            },
        )
    }

    fn navigation_input(&self, ctx: &mut EventCtx<'_>, keyboard: bool) {
        if self.current_page.is_some() || self.controls.is_some() {
            ctx.submit_untyped_action(Box::new(super::NavigationInput { keyboard }));
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
                self.navigation_input(ctx, false);
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
        if matches!(event, TextEvent::Keyboard(_)) {
            self.navigation_input(ctx, true);
        }
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
            self.navigation_input(ctx, true);
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        self.focus.update(event);
        if matches!(event, Update::WidgetAdded)
            && let Some(identity) = self.navigation_identity()
        {
            ctx.submit_untyped_action(Box::new(identity));
        }
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

    fn property_changed(
        &mut self,
        ctx: &mut xilem::masonry::core::UpdateCtx<'_>,
        property_type: std::any::TypeId,
    ) {
        components::Palette::prop_changed(ctx, property_type);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let palette = *props.get::<components::Palette>(ctx.property_cache());
        if self.selected.is_some() || self.current_page.is_some() || self.controls.is_some() {
            // Cards and navigation actions use one enclosing material/focus
            // surface around their complete native action.
            return;
        }
        components::settings_material::paint_button_with_palette(
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
            &palette,
        );
        if ctx.is_focus_target() && self.focus.visible() {
            painter
                .stroke(
                    ctx.border_box()
                        .inset(-3.0)
                        .to_rounded_rect(components::RADIUS_CONTROL),
                    &Stroke::new(2.0),
                    palette.focus_ring,
                )
                .draw();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.name.clone());
        if let Some(selected) = self.selected {
            node.set_toggled(if selected {
                Toggled::True
            } else {
                Toggled::False
            });
        } else if let Some(current) = self.current_page {
            node.set_selected(current);
            if current {
                node.set_aria_current(AriaCurrent::Page);
            }
        } else {
            node.set_expanded(self.expanded);
        }
        if let Some(target) = self.controls
            && self.expanded
        {
            node.set_controls([target.into()]);
        }
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
