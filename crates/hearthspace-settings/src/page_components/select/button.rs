use super::layer::SettingsSelectLayer;
use hearthspace_ui_components as components;
use xilem::masonry::{
    accesskit::{Action, Node, Role},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, ComposeCtx, EventCtx, LayerType, LayoutCtx,
        MeasureCtx, NewWidget, PaintCtx, PointerButton, PointerEvent, PropertiesMut, PropertiesRef,
        RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
        keyboard::{Key, NamedKey},
    },
    imaging::Painter,
    kurbo::{Axis, Point, Rect, Size},
    layout::{LayoutSize, LenReq, Length},
};

struct Popup {
    layer: WidgetId,
    options: Vec<WidgetId>,
    active: usize,
    anchor: Rect,
}

pub(super) struct SettingsSelectButton {
    pub name: String,
    pub options: Vec<String>,
    pub selected: usize,
    child: WidgetPod<dyn Widget>,
    popup: Option<Popup>,
    focus: components::settings_material::FocusIndicator,
}

impl SettingsSelectButton {
    pub fn new(
        name: String,
        options: Vec<String>,
        selected: usize,
        child: NewWidget<impl Widget + ?Sized>,
    ) -> Self {
        Self {
            name,
            options,
            selected,
            child: child.erased().to_pod(),
            popup: None,
            focus: components::settings_material::FocusIndicator::default(),
        }
    }

    pub fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    pub fn dismiss(this: &mut WidgetMut<'_, Self>) {
        if let Some(popup) = this.widget.popup.take() {
            this.ctx.remove_layer(popup.layer);
            this.ctx.request_accessibility_update();
            this.ctx.request_paint_only();
        }
    }

    pub fn finish(this: &mut WidgetMut<'_, Self>, selection: Option<usize>) {
        if this.widget.popup.is_none() {
            return;
        }
        Self::dismiss(this);
        if let Some(index) = selection {
            this.ctx.submit_action::<usize>(index);
        }
    }

    pub fn update_anchor(this: &mut WidgetMut<'_, Self>) {
        // bounding_box includes the shadow/focus paint insets; menus attach to
        // the actual control border, expressed in window coordinates.
        let anchor = this
            .ctx
            .window_transform()
            .transform_rect_bbox(this.ctx.border_box());
        if let Some(popup) = &mut this.widget.popup
            && popup.anchor != anchor
        {
            popup.anchor = anchor;
            this.ctx.mutate_later(popup.layer, move |mut layer| {
                SettingsSelectLayer::set_anchor(&mut layer.downcast(), anchor);
            });
        }
    }

    fn open(&mut self, ctx: &mut EventCtx<'_>) {
        if ctx.is_disabled() || self.popup.is_some() {
            return;
        }
        let anchor = ctx.window_transform().transform_rect_bbox(ctx.border_box());
        let layer = SettingsSelectLayer::new(
            ctx.widget_id(),
            anchor,
            self.name.clone(),
            &self.options,
            self.selected,
        );
        let options = layer.option_ids();
        let layer = NewWidget::new(layer);
        self.popup = Some(Popup {
            layer: layer.id(),
            options,
            active: self.selected,
            anchor,
        });
        ctx.create_attached_layer(LayerType::Other, layer, Point::ORIGIN);
        ctx.request_focus();
        ctx.request_accessibility_update();
        ctx.request_paint_only();
    }

    fn close(&mut self, ctx: &mut EventCtx<'_>, selection: Option<usize>) {
        if let Some(popup) = self.popup.take() {
            ctx.remove_layer(popup.layer);
            ctx.request_accessibility_update();
            ctx.request_paint_only();
            if let Some(index) = selection {
                ctx.submit_action::<usize>(index);
            }
        }
    }

    fn toggle(&mut self, ctx: &mut EventCtx<'_>) {
        if self.popup.is_some() {
            self.close(ctx, None);
        } else {
            self.open(ctx);
        }
    }

    fn navigate(&mut self, ctx: &mut EventCtx<'_>, key: NamedKey) {
        if self.popup.is_none() {
            self.open(ctx);
            ctx.set_handled();
            return;
        }
        let Some(popup) = &mut self.popup else {
            return;
        };
        popup.active = match key {
            NamedKey::ArrowUp => popup.active.saturating_sub(1),
            NamedKey::ArrowDown => (popup.active + 1).min(self.options.len() - 1),
            NamedKey::Home => 0,
            NamedKey::End => self.options.len() - 1,
            _ => popup.active,
        };
        let active = popup.active;
        ctx.mutate_later(popup.layer, move |mut layer| {
            SettingsSelectLayer::highlight(&mut layer.downcast(), active);
        });
        ctx.request_accessibility_update();
        ctx.set_handled();
    }
}

impl Widget for SettingsSelectButton {
    type Action = usize;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.focus.pointer(event);
        match event {
            PointerEvent::Down(event)
                if matches!(event.button, None | Some(PointerButton::Primary)) =>
            {
                ctx.request_focus();
                ctx.capture_pointer();
                ctx.set_handled();
            }
            PointerEvent::Up(event)
                if matches!(event.button, None | Some(PointerButton::Primary))
                    && ctx.is_active()
                    && ctx.is_hovered() =>
            {
                self.toggle(ctx);
                ctx.set_handled();
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
        match event {
            TextEvent::WindowFocusChange(false) => self.close(ctx, None),
            TextEvent::Keyboard(event) => {
                if event.state.is_down() {
                    match event.key {
                        Key::Named(
                            key @ (NamedKey::ArrowUp
                            | NamedKey::ArrowDown
                            | NamedKey::Home
                            | NamedKey::End),
                        ) => {
                            self.navigate(ctx, key);
                        }
                        Key::Named(NamedKey::Escape) if self.popup.is_some() => {
                            self.close(ctx, None);
                            ctx.set_handled();
                        }
                        Key::Named(NamedKey::Tab) => self.close(ctx, None),
                        _ => {}
                    }
                } else if event.key == Key::Named(NamedKey::Enter)
                    || matches!(&event.key, Key::Character(value) if value == " ")
                {
                    if let Some(popup) = &self.popup {
                        self.close(ctx, Some(popup.active));
                    } else {
                        self.open(ctx);
                    }
                    ctx.set_handled();
                }
            }
            _ => {}
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        match event.action {
            Action::Click => self.toggle(ctx),
            Action::Expand => self.open(ctx),
            Action::Collapse => self.close(ctx, None),
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        self.focus.update(event);
        if matches!(
            event,
            Update::DisabledChanged(true)
                | Update::StashedChanged(true)
                | Update::FocusChanged(false)
        ) && let Some(popup) = self.popup.take()
        {
            ctx.remove_layer(popup.layer);
            ctx.request_accessibility_update();
        }
        if matches!(
            event,
            Update::FocusChanged(_) | Update::HoveredChanged(_) | Update::ActiveChanged(_)
        ) {
            ctx.request_paint_only();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
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
    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        if self.popup.is_some() {
            ctx.mutate_self_later(|mut this| Self::update_anchor(&mut this.downcast()));
        }
    }
    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        components::settings_material::paint_button(
            painter,
            ctx.border_box(),
            components::RADIUS_CONTROL,
            components::SettingsButton::Control,
            false,
            components::settings_material::Interaction {
                hovered: ctx.is_hovered(),
                active: ctx.is_active(),
                focused: ctx.is_focus_target() && self.focus.visible(),
                disabled: ctx.is_disabled(),
            },
        );
    }
    fn accessibility_role(&self) -> Role {
        Role::ComboBox
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.name.clone());
        node.set_value(self.options[self.selected].clone());
        node.set_expanded(self.popup.is_some());
        node.add_action(Action::Click);
        node.add_action(Action::Expand);
        node.add_action(Action::Collapse);
        if let Some(popup) = &self.popup {
            node.set_controls([popup.layer.to_raw().into()]);
            node.set_active_descendant(popup.options[popup.active].to_raw().into());
        }
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
