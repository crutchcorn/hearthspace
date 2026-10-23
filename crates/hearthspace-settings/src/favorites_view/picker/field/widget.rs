use super::*;

impl Widget for PickerField {
    type Action = PickerAction;

    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if source == self.trigger.id() && action.is::<ButtonPress>() {
            ctx.set_focus(self.editor.id());
            if self.popup.is_some() {
                ctx.mutate_self_later(|mut this| Self::finish(&mut this.downcast(), None));
            } else {
                ctx.submit_action::<PickerAction>(PickerAction::Open);
            }
            ctx.set_handled();
        } else if source == self.editor.id() {
            if let Some(action) = action.downcast_ref::<TextAction>() {
                match action {
                    TextAction::Changed(query) => {
                        ctx.submit_action::<PickerAction>(PickerAction::Query(query.clone()))
                    }
                    TextAction::Cancelled => {
                        ctx.mutate_self_later(|mut this| Self::finish(&mut this.downcast(), None))
                    }
                    TextAction::Entered(_) => {
                        if let Some(choice) = self
                            .popup
                            .as_ref()
                            .and_then(|popup| self.choices.get(popup.active))
                        {
                            let id = choice.id.clone();
                            ctx.mutate_self_later(move |mut this| {
                                Self::finish(&mut this.downcast(), Some(id))
                            });
                        }
                    }
                }
                ctx.set_handled();
            } else if let Some(navigation) = action.downcast_ref::<Navigation>() {
                match navigation {
                    Navigation::Focus => ctx.set_focus(self.editor.id()),
                    Navigation::Collapse => {
                        ctx.mutate_self_later(|mut this| Self::finish(&mut this.downcast(), None))
                    }
                    Navigation::Expand => ctx.submit_action::<PickerAction>(PickerAction::Open),
                    Navigation::Step(step) => {
                        if self.popup.is_some() {
                            let step = *step;
                            ctx.mutate_self_later(move |mut this| {
                                Self::navigate(&mut this.downcast(), step)
                            });
                        } else {
                            ctx.submit_action::<PickerAction>(PickerAction::Open);
                        }
                    }
                }
                ctx.set_handled();
            }
        }
    }
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        ctx.request_paint_only();
        if matches!(event, PointerEvent::Down(_)) {
            ctx.set_focus(self.editor.id());
        }
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        ctx.request_paint_only();
        if matches!(event, TextEvent::WindowFocusChange(false))
            || matches!(event, TextEvent::Keyboard(key) if key.state.is_down() && key.key == Key::Named(NamedKey::Tab))
        {
            ctx.mutate_self_later(|mut this| Self::finish(&mut this.downcast(), None));
        }
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded) {
            ctx.mutate_child_later(&mut self.editor, |mut editor| {
                editor
                    .ctx
                    .submit_untyped_action(Box::new(Navigation::Focus));
            });
        }
        if matches!(
            event,
            Update::ChildFocusChanged(false)
                | Update::StashedChanged(true)
                | Update::DisabledChanged(true)
        ) && self.popup.is_some()
        {
            ctx.mutate_self_later(|mut this| Self::finish(&mut this.downcast(), None));
        }
        if matches!(
            event,
            Update::ChildFocusChanged(_) | Update::HoveredChanged(_)
        ) {
            ctx.request_paint_only();
        }
    }
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.artwork);
        ctx.register_child(&mut self.editor);
        ctx.register_child(&mut self.trigger);
    }
    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        _: Option<Length>,
    ) -> Length {
        match (axis, request) {
            (Axis::Horizontal, LenReq::FitContent(space)) => space,
            (Axis::Horizontal, _) => 90.0.px(),
            _ => 46.0.px(),
        }
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.set_paint_insets(24.0);
        ctx.run_layout(&mut self.artwork, Size::new(24.0, 24.0));
        ctx.place_child(
            &mut self.artwork,
            Point::new(11.0, (size.height - 24.0) / 2.0),
        );
        ctx.run_layout(
            &mut self.trigger,
            Size::new(44.0, (size.height - 2.0).max(0.0)),
        );
        ctx.place_child(
            &mut self.trigger,
            Point::new((size.width - 45.0).max(0.0), 1.0),
        );
        let width = (size.width - 90.0).max(0.0);
        let height = ctx
            .compute_length(
                &mut self.editor,
                LenReq::MinContent.into(),
                xilem::masonry::layout::LayoutSize::maybe(Axis::Horizontal, Some(width.px())),
                Axis::Vertical,
                Some(width.px()),
            )
            .get();
        let height = height.min(size.height - 2.0).max(0.0);
        ctx.run_layout(&mut self.editor, Size::new(width, height));
        ctx.place_child(
            &mut self.editor,
            Point::new(45.0, (size.height - height) / 2.0),
        );
    }
    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        if self.popup.is_some() {
            ctx.mutate_self_later(|mut this| Self::update_anchor(&mut this.downcast()));
        }
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
        let rect = ctx.border_box();
        // The reference moves its visual focus into the open menu. Retain the
        // editor's native text focus, but show its field outline only when closed.
        let focused = ctx.has_focus_target() && self.popup.is_none();
        if focused {
            components::settings_material::shadow(
                painter,
                rect,
                10.0,
                palette.control_shadow,
                (0.0, 6.0),
                22.0,
                -5.0,
            );
        }
        let shape = rect.inset(-0.5).to_rounded_rect(9.5);
        painter.fill(shape, palette.surface_control).draw();
        if focused {
            let wash = xilem::masonry::peniko::Gradient::new_radial(
                rect.center(),
                (rect.width().max(rect.height()) / 2.0) as f32,
            )
            .with_stops([
                (0.0, xilem::Color::TRANSPARENT),
                (1.0, palette.control_inset_glow),
            ]);
            painter.fill(shape, &wash).draw();
        }
        painter
            .stroke(shape, &Stroke::new(1.0), palette.chrome_border)
            .draw();
        components::settings_material::glass_edge_with_palette(
            painter, rect, 10.0, focused, &palette,
        );
        if focused {
            painter
                .stroke(
                    rect.inset(-1.0).to_rounded_rect(9.0),
                    &Stroke::new(2.0),
                    palette.accent,
                )
                .draw();
        }
    }
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.artwork.id(), self.editor.id(), self.trigger.id()])
    }
}
