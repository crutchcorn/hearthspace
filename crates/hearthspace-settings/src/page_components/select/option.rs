use super::button::SettingsSelectButton;
use hearthspace_ui_components as components;
use xilem::masonry::{
    accesskit::{Action, Node, Role},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NoAction, PaintCtx,
        PointerButton, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx,
        Widget, WidgetId, WidgetPod,
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size, Stroke},
    layout::{LayoutSize, LenReq, Length, SizeDef},
    parley::{
        FontFamilyName,
        style::{FontFamily, StyleProperty},
    },
    widgets::Label,
};

pub(super) struct SelectOption {
    creator: WidgetId,
    index: usize,
    name: String,
    selected: bool,
    pub highlighted: bool,
    label: WidgetPod<Label>,
}

impl SelectOption {
    pub fn new(creator: WidgetId, index: usize, name: String, selected: bool) -> Self {
        Self {
            creator,
            index,
            name: name.clone(),
            selected,
            highlighted: selected,
            label: WidgetPod::new(
                Label::new(name)
                    .with_style(StyleProperty::FontSize(11.0))
                    .with_style(StyleProperty::FontFamily(FontFamily::Single(
                        FontFamilyName::Named(crate::typography::FONT.into()),
                    ))),
            ),
        }
    }

    fn choose(&self, ctx: &mut EventCtx<'_>) {
        let index = self.index;
        ctx.set_focus(self.creator);
        ctx.mutate_later(self.creator, move |mut creator| {
            SettingsSelectButton::finish(&mut creator.downcast(), Some(index));
        });
        ctx.set_handled();
    }
}

impl Widget for SelectOption {
    type Action = NoAction;
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(event)
                if matches!(event.button, None | Some(PointerButton::Primary)) =>
            {
                // Masonry clears focus before dispatching pointer-down on a
                // different layer. Keep the combobox owner focused throughout
                // the press, so its blur handler does not dismiss before Up.
                ctx.set_focus(self.creator);
                ctx.capture_pointer();
                ctx.set_handled();
            }
            PointerEvent::Up(event)
                if matches!(event.button, None | Some(PointerButton::Primary))
                    && ctx.is_active()
                    && ctx.is_hovered() =>
            {
                self.choose(ctx)
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
        if event.action == Action::Click {
            self.choose(ctx);
        }
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::HoveredChanged(_) | Update::ActiveChanged(_)) {
            ctx.request_paint_only();
        }
    }
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.label);
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
            &mut self.label,
            request.into(),
            LayoutSize::maybe(axis.cross(), cross),
            axis,
            cross,
        )
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        let available = Size::new((size.width - 16.0).max(0.0), size.height);
        let measured = ctx.compute_size(&mut self.label, SizeDef::fit(available), available.into());
        ctx.run_layout(&mut self.label, measured);
        ctx.place_child(
            &mut self.label,
            Point::new(8.0, (size.height - measured.height) / 2.0),
        );
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
        let bounds = ctx.border_box().inset(-1.0);
        components::settings_material::paint_button_with_palette(
            painter,
            bounds,
            components::RADIUS_DETAIL,
            components::SettingsButton::Bare,
            self.selected,
            components::settings_material::Interaction {
                hovered: ctx.is_hovered(),
                active: ctx.is_active(),
                focused: false,
                disabled: ctx.is_disabled(),
            },
            &palette,
        );
        if self.highlighted {
            painter
                .stroke(
                    bounds.to_rounded_rect(components::RADIUS_DETAIL),
                    &Stroke::new(2.0),
                    palette.focus_ring,
                )
                .draw();
        }
    }
    fn accessibility_role(&self) -> Role {
        Role::ListBoxOption
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.name.clone());
        node.set_selected(self.selected);
        node.add_action(Action::Click);
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.label.id()])
    }
    fn propagates_pointer_interaction(&self) -> bool {
        false
    }
}
