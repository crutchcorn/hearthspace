//! Input and accessibility for tray buttons; painting is deliberately independent of D-Bus.
use crate::tray::TrayToggle;
use hearthspace_ui_components as components;
use xilem::masonry::{
    accesskit::{Action, Node, Role, Toggled},
    core::{
        AccessCtx, AccessEvent, ActionCtx, ChildrenIds, ErasedAction, EventCtx, LayoutCtx,
        MeasureCtx, PaintCtx, PointerButton, PointerEvent, PropertiesMut, PropertiesRef,
        RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetPod,
        keyboard::{Key, NamedKey},
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size},
    layout::{AsUnit, LenReq, Length},
    peniko::ImageBrush,
    properties::LineBreaking,
    widgets::Label,
};

mod touch;

#[derive(Clone, PartialEq)]
pub(super) struct Appearance {
    pub name: String,
    pub description: String,
    pub image: Option<ImageBrush>,
    pub icon: bool,
    pub scroll_in_parent: bool,
    pub attention: bool,
    pub enabled: bool,
    pub selected: bool,
    pub autofocus: bool,
    pub menu: bool,
    pub separator: bool,
    pub submenu: bool,
    pub toggle: Option<TrayToggle>,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            image: None,
            icon: false,
            scroll_in_parent: false,
            attention: false,
            enabled: true,
            selected: false,
            autofocus: false,
            menu: false,
            separator: false,
            submenu: false,
            toggle: None,
        }
    }
}
#[derive(Debug)]
pub(super) enum ButtonAction {
    Activate,
    Context,
    Secondary,
    Scroll { delta: i32, horizontal: bool },
    Hover(bool),
    Dismiss,
    Step(i32),
    Back,
    Focus,
}
#[derive(Debug)]
pub(super) struct Reveal;
#[derive(Debug)]
pub(super) struct ButtonEvent {
    pub target: super::tray_button::Target,
    pub action: ButtonAction,
}
pub(super) struct TrayButton {
    pub appearance: Appearance,
    pub cancelled_press: bool,
    pub target: super::tray_button::Target,
    pressed: Option<Option<PointerButton>>,
    touch_hold: Option<touch::Hold>,
    pub text: Option<WidgetPod<Label>>,
}
impl TrayButton {
    fn submit(&self, ctx: &mut EventCtx<'_>, action: ButtonAction) {
        ctx.submit_action::<ButtonEvent>(ButtonEvent {
            target: self.target.clone(),
            action,
        });
    }
    pub fn new(appearance: Appearance, target: super::tray_button::Target) -> Self {
        let text = (!appearance.icon && !appearance.separator).then(|| {
            Label::new(appearance.name.clone())
                .prepare()
                .with_props(LineBreaking::Clip)
                .with_class(if appearance.enabled {
                    "shell-text-shell-ink"
                } else {
                    "shell-muted-shell-ink"
                })
                .to_pod()
        });
        Self {
            appearance,
            text,
            cancelled_press: false,
            target,
            pressed: None,
            touch_hold: None,
        }
    }
}
impl Widget for TrayButton {
    type Action = ButtonEvent;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        if let Some(text) = &mut self.text {
            ctx.register_child(text);
        }
    }
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.track_touch(event, ctx);
        match event {
            PointerEvent::Down(event) => {
                self.pressed = Some(event.button);
                self.cancelled_press = false;
                ctx.capture_pointer();
                ctx.request_focus();
                ctx.request_paint_only();
            }
            PointerEvent::Up(event)
                if ctx.is_active()
                    && ctx.is_hovered()
                    && !self.cancelled_press
                    && self.pressed == Some(event.button) =>
            {
                self.submit(
                    ctx,
                    match event.button {
                        Some(PointerButton::Secondary) => ButtonAction::Context,
                        Some(PointerButton::Auxiliary) => ButtonAction::Secondary,
                        _ => ButtonAction::Activate,
                    },
                );
                ctx.request_paint_only();
            }
            PointerEvent::Scroll(event)
                if self.appearance.icon && !self.appearance.scroll_in_parent =>
            {
                let unit = xilem::dpi::PhysicalPosition::new(120.0, 120.0);
                let delta = event.delta.to_pixel_delta(unit, unit);
                for (value, horizontal) in [(delta.x, true), (delta.y, false)] {
                    if value.is_finite() && value != 0.0 {
                        self.submit(
                            ctx,
                            ButtonAction::Scroll {
                                delta: value.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32,
                                horizontal,
                            },
                        );
                    }
                }
                ctx.set_handled();
            }
            _ => {}
        }
    }
    fn on_anim_frame(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, interval: u64) {
        self.advance_touch_hold(ctx, interval);
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if matches!(event, TextEvent::WindowFocusChange(false)) {
            self.touch_hold = None;
            self.cancelled_press = true;
        }
        let TextEvent::Keyboard(event) = event else {
            return;
        };
        if !event.state.is_down() {
            return;
        }
        let action = match &event.key {
            Key::Named(NamedKey::ContextMenu) => Some(ButtonAction::Context),
            Key::Named(NamedKey::F10) if event.modifiers.shift() => Some(ButtonAction::Context),
            Key::Named(NamedKey::Escape) => Some(ButtonAction::Dismiss),
            Key::Named(NamedKey::ArrowUp) if !self.appearance.icon => Some(ButtonAction::Step(-1)),
            Key::Named(NamedKey::ArrowDown) if !self.appearance.icon => Some(ButtonAction::Step(1)),
            Key::Named(NamedKey::ArrowLeft) if !self.appearance.icon => Some(ButtonAction::Back),
            Key::Named(NamedKey::ArrowRight) if self.appearance.submenu => {
                Some(ButtonAction::Activate)
            }
            Key::Named(NamedKey::Enter) => Some(ButtonAction::Activate),
            Key::Character(key) if key == " " => Some(ButtonAction::Activate),
            _ => None,
        };
        if let Some(action) = action {
            self.submit(ctx, action);
            ctx.set_handled();
        }
    }
    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        match event.action {
            Action::Click => self.submit(ctx, ButtonAction::Activate),
            Action::ShowContextMenu => self.submit(ctx, ButtonAction::Context),
            _ => {}
        }
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::HoveredChanged(hovered) = event {
            let hovered = *hovered;
            let target = self.target.clone();
            ctx.mutate_self_later(move |mut widget| {
                widget.ctx.submit_action::<ButtonEvent>(ButtonEvent {
                    target,
                    action: ButtonAction::Hover(hovered),
                })
            });
            ctx.request_paint_only();
        }
        if matches!(event, Update::WidgetAdded)
            && (self.appearance.selected || self.appearance.autofocus)
            && let Some(text) = &mut self.text
        {
            ctx.mutate_child_later(text, |mut child| {
                child.ctx.submit_untyped_action(Box::new(Reveal))
            });
        }
        if matches!(event, Update::FocusChanged(_)) {
            ctx.request_paint_only();
        }
        if matches!(event, Update::FocusChanged(true)) {
            let target = self.target.clone();
            ctx.mutate_self_later(move |mut widget| {
                widget.ctx.submit_action::<ButtonEvent>(ButtonEvent {
                    target,
                    action: ButtonAction::Focus,
                })
            });
            ctx.request_paint_only();
        }
    }
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if self.text.as_ref().is_some_and(|text| text.id() == source) && action.is::<Reveal>() {
            ctx.set_focus(ctx.widget_id());
            ctx.request_scroll_to_this();
            ctx.set_handled();
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
        if self.appearance.separator {
            return 1.0.px();
        }
        if self.appearance.icon || axis == Axis::Vertical {
            return 34.0.px();
        }
        if let Some(text) = &mut self.text {
            ctx.redirect_measurement(text, axis, cross)
                .saturating_add(48.0.px())
        } else {
            34.0.px()
        }
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        if let Some(text) = &mut self.text {
            ctx.run_layout(text, Size::new((size.width - 48.0).max(0.0), size.height));
            ctx.place_child(text, Point::new(24.0, 0.0));
        }
    }
    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property: std::any::TypeId) {
        components::Palette::prop_changed(ctx, property);
    }
    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let palette = *props.get::<components::Palette>(ctx.property_cache());
        super::tray_paint::paint(&self.appearance, ctx, painter, &palette);
    }
    fn accessibility_role(&self) -> Role {
        if self.appearance.separator {
            return Role::Splitter;
        }
        if self.appearance.menu {
            match self.appearance.toggle {
                Some(TrayToggle::Check(_) | TrayToggle::Indeterminate) => Role::MenuItemCheckBox,
                Some(TrayToggle::Radio(_)) => Role::MenuItemRadio,
                None => Role::MenuItem,
            }
        } else {
            Role::Button
        }
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        if self.appearance.separator {
            return;
        }
        node.set_label(self.appearance.name.clone());
        if !self.appearance.description.is_empty() {
            node.set_description(self.appearance.description.clone());
        }
        if let Some(toggle) = self.appearance.toggle {
            node.set_toggled(match toggle {
                TrayToggle::Check(true) | TrayToggle::Radio(true) => Toggled::True,
                TrayToggle::Check(false) | TrayToggle::Radio(false) => Toggled::False,
                TrayToggle::Indeterminate => Toggled::Mixed,
            });
        }
        if self.appearance.menu {
            node.set_selected(self.appearance.selected);
        }
        if self.appearance.icon || self.appearance.submenu {
            node.set_has_popup(xilem::masonry::accesskit::HasPopup::Menu);
        }
        node.add_action(Action::Click);
        if self.appearance.icon {
            node.add_action(Action::ShowContextMenu);
        }
    }
    fn accepts_focus(&self) -> bool {
        !self.appearance.separator
    }
    fn propagates_pointer_interaction(&self) -> bool {
        false
    }
    fn children_ids(&self) -> ChildrenIds {
        self.text.as_ref().map_or_else(ChildrenIds::new, |text| {
            ChildrenIds::from_slice(&[text.id()])
        })
    }
    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("TrayButton", id = id.trace())
    }
}
