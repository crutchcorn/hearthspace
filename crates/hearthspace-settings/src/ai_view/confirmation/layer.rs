use super::{
    Decision,
    owner::ConfirmationOwner,
    panel::{ConfirmationPanel, FocusKeep},
};
use std::{cell::Cell, rc::Rc};
use xilem::{
    Color,
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ActionCtx, ChildrenIds, ErasedAction, EventCtx, Layer, LayoutCtx,
            MeasureCtx, NoAction, PaintCtx, PointerEvent, PropertiesMut, PropertiesRef,
            RegisterCtx, TextEvent, Widget, WidgetId, WidgetPod,
            keyboard::{Key, NamedKey},
        },
        imaging::Painter,
        kurbo::{Axis, Point, Rect, Size},
        layout::{LenReq, Length, SizeDef},
        widgets::ButtonPress,
    },
};

pub(super) struct ConfirmationLayer {
    creator: WidgetId,
    panel: WidgetPod<ConfirmationPanel>,
    actions: [WidgetId; 2],
    bounds: Rect,
    closing: bool,
    keyboard: Rc<Cell<bool>>,
}

impl ConfirmationLayer {
    pub fn new(creator: WidgetId, keyboard: bool) -> Self {
        let keyboard = Rc::new(Cell::new(keyboard));
        let panel = ConfirmationPanel::new(keyboard.clone());
        let actions = panel.actions();
        Self {
            creator,
            panel: WidgetPod::new(panel),
            actions,
            bounds: Rect::ZERO,
            closing: false,
            keyboard,
        }
    }

    fn choose(&mut self, ctx: &mut EventCtx<'_>, decision: Decision) {
        if !self.closing {
            self.closing = true;
            ctx.mutate_later(self.creator, move |mut owner| {
                let mut owner = owner.downcast::<ConfirmationOwner>();
                owner.ctx.submit_action::<Decision>(decision);
            });
        }
        ctx.set_handled();
    }

    fn set_keyboard(&mut self, ctx: &mut EventCtx<'_>, keyboard: bool) {
        if self.keyboard.replace(keyboard) != keyboard {
            ctx.mutate_child_later(&mut self.panel, |mut panel| {
                ConfirmationPanel::repaint_actions(&mut panel);
            });
        }
    }
}

impl Widget for ConfirmationLayer {
    type Action = NoAction;
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Down(event) = event {
            self.set_keyboard(ctx, false);
            if !self
                .bounds
                .contains(ctx.local_position(event.state.position))
            {
                self.choose(ctx, Decision::Keep);
            } else if !self.actions.contains(&ctx.target()) {
                // Pressing the heading must not leave the modal without a focus
                // target, which would allow Tab to enter the underlying page.
                ctx.set_focus(self.actions[0]);
                ctx.set_handled();
            }
        }
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Keyboard(event) = event
            && event.state.is_down()
        {
            self.set_keyboard(ctx, true);
            match event.key {
                Key::Named(NamedKey::Escape) => self.choose(ctx, Decision::Keep),
                Key::Named(NamedKey::Tab) => {
                    // A two-action dialog cycles in either direction. Focus can
                    // only leave after a safe cancellation or confirmed delete.
                    let next = if ctx.focus_target_id() == Some(self.actions[0]) {
                        self.actions[1]
                    } else {
                        self.actions[0]
                    };
                    ctx.set_focus(next);
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }
    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if source == self.actions[0] && action.is::<FocusKeep>() {
            ctx.set_focus(source);
            ctx.set_handled();
        } else if let Some(index) = self.actions.iter().position(|id| *id == source)
            && action.is::<ButtonPress>()
        {
            if !self.closing {
                self.closing = true;
                let decision = if index == 0 {
                    Decision::Keep
                } else {
                    Decision::Delete
                };
                ctx.mutate_later(self.creator, move |mut owner| {
                    let mut owner = owner.downcast::<ConfirmationOwner>();
                    owner.ctx.submit_action::<Decision>(decision);
                });
            }
            ctx.set_handled();
        }
    }
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.panel);
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        _: Option<Length>,
    ) -> Length {
        ctx.context_size().length(axis).unwrap_or(Length::ZERO)
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        let available = Size::new(
            (size.width - 32.0).clamp(0.0, 380.0),
            (size.height - 16.0).max(0.0),
        );
        let panel = ctx.compute_size(&mut self.panel, SizeDef::fit(available), available.into());
        self.bounds = Rect::from_origin_size(
            Point::new(
                (size.width - panel.width) / 2.0,
                ((size.height - panel.height) / 2.0).max(8.0),
            ),
            panel,
        );
        ctx.run_layout(&mut self.panel, panel);
        ctx.place_child(&mut self.panel, self.bounds.origin());
    }
    fn property_changed(
        &mut self,
        ctx: &mut xilem::masonry::core::UpdateCtx<'_>,
        property_type: std::any::TypeId,
    ) {
        hearthspace_ui_components::Palette::prop_changed(ctx, property_type);
    }
    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let palette = *props.get::<hearthspace_ui_components::Palette>(ctx.property_cache());
        let [red, green, blue, _] = palette.app_background.to_rgba8().to_u8_array();
        painter
            // color-mix(in srgb, var(--field) 30%, #0005), including alpha.
            .fill(
                ctx.border_box(),
                Color::from_rgba8(
                    (f32::from(red) * 0.5625).round() as u8,
                    (f32::from(green) * 0.5625).round() as u8,
                    (f32::from(blue) * 0.5625).round() as u8,
                    136,
                ),
            )
            .draw();
    }
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.panel.id()])
    }
    fn as_layer(&mut self) -> Option<&mut dyn Layer> {
        Some(self)
    }
}

impl Layer for ConfirmationLayer {
    fn capture_pointer_event(
        &mut self,
        _: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        _: &PointerEvent,
    ) {
    }
}
