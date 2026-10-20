use super::Choice;
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, IconWidget, icon},
    settings_material::{Interaction, paint_button_with_palette},
};
use xilem::masonry::{
    accesskit::{Action, Node, Role},
    core::{
        AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NoAction, PaintCtx,
        PointerButton, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx,
        Widget, WidgetId, WidgetPod,
    },
    imaging::Painter,
    kurbo::{Affine, Axis, Point, Size},
    layout::{AsUnit, LayoutSize, LenReq, Length, SizeDef},
    parley::{FontFamily, FontFamilyName, StyleProperty},
    peniko::ImageBrush,
    properties::LineBreaking,
    widgets::{ButtonPress, Label},
};

pub(super) struct Artwork {
    pub image: Option<ImageBrush>,
    fallback: IconWidget,
}
impl Artwork {
    pub fn new(image: Option<ImageBrush>) -> Self {
        Self {
            image,
            fallback: icon(Icon::AppsGrid, 24.0).into_widget(),
        }
    }
}
impl Widget for Artwork {
    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, _: std::any::TypeId) {
        ctx.request_paint_only();
    }
    type Action = NoAction;
    fn register_children(&mut self, _: &mut RegisterCtx<'_>) {}
    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        _: Axis,
        _: LenReq,
        _: Option<Length>,
    ) -> Length {
        24.0.px()
    }
    fn layout(&mut self, _: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, _: Size) {}
    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        if let Some(image) = &self.image {
            let size = ctx.content_box().size();
            let scale = (size.width / image.image.width as f64)
                .min(size.height / image.image.height as f64);
            painter.draw_image(
                image,
                Affine::translate((
                    (size.width - image.image.width as f64 * scale) / 2.0,
                    (size.height - image.image.height as f64 * scale) / 2.0,
                )) * Affine::scale(scale),
            );
        } else {
            self.fallback.paint(ctx, props, painter);
        }
    }
    fn accessibility_role(&self) -> Role {
        Role::Image
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_hidden();
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

pub(super) struct AppOption {
    editor: WidgetId,
    name: String,
    selected: bool,
    pub highlighted: bool,
    artwork: WidgetPod<Artwork>,
    label: WidgetPod<Label>,
    check: WidgetPod<IconWidget>,
}
impl AppOption {
    pub fn new(editor: WidgetId, choice: &Choice, selected: bool, highlighted: bool) -> Self {
        Self {
            editor,
            name: choice.name.clone(),
            selected,
            highlighted,
            artwork: WidgetPod::new(Artwork::new(choice.image.clone())),
            label: Label::new(choice.name.clone())
                .with_style(StyleProperty::FontSize(12.0))
                .with_style(FontFamily::Single(FontFamilyName::Named(
                    crate::typography::FONT.into(),
                )))
                .prepare()
                .with_props(LineBreaking::Clip)
                .with_class(if selected {
                    "settings-accent"
                } else {
                    "settings-text"
                })
                .to_pod(),
            check: icon(Icon::Check, 16.0)
                .into_widget()
                .prepare()
                .with_class("settings-accent")
                .to_pod(),
        }
    }
}
impl Widget for AppOption {
    type Action = ButtonPress;
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
                ctx.set_focus(self.editor);
                ctx.capture_pointer();
            }
            PointerEvent::Up(event)
                if matches!(event.button, None | Some(PointerButton::Primary))
                    && ctx.is_active()
                    && ctx.is_hovered() =>
            {
                ctx.submit_action::<ButtonPress>(ButtonPress {
                    button: event.button,
                });
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
            ctx.set_focus(self.editor);
            ctx.submit_action::<ButtonPress>(ButtonPress { button: None });
        }
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::HoveredChanged(_) | Update::ActiveChanged(_)) {
            ctx.request_paint_only();
        }
    }
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.artwork);
        ctx.register_child(&mut self.label);
        ctx.register_child(&mut self.check);
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
            _ => 44.0.px(),
        }
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.artwork, Size::new(24.0, 24.0));
        ctx.place_child(
            &mut self.artwork,
            Point::new(10.0, (size.height - 24.0) / 2.0),
        );
        let available = Size::new((size.width - 70.0).max(0.0), size.height);
        let height = ctx
            .compute_length(
                &mut self.label,
                LenReq::MinContent.into(),
                LayoutSize::maybe(Axis::Horizontal, Some(available.width.px())),
                Axis::Vertical,
                Some(available.width.px()),
            )
            .get();
        ctx.run_layout(&mut self.label, Size::new(available.width, height));
        ctx.place_child(
            &mut self.label,
            Point::new(44.0, (size.height - height) / 2.0),
        );
        ctx.set_stashed(&mut self.check, !self.selected);
        if self.selected {
            let measured = ctx.compute_size(
                &mut self.check,
                SizeDef::fit(Size::new(16.0, 16.0)),
                Size::new(16.0, 16.0).into(),
            );
            ctx.run_layout(&mut self.check, measured);
            ctx.place_child(
                &mut self.check,
                Point::new((size.width - 26.0).max(0.0), (size.height - 16.0) / 2.0),
            );
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
        if self.selected {
            let rect = ctx.border_box();
            painter
                .fill(rect.to_rounded_rect(9.0), palette.accent_soft)
                .draw();
            components::settings_material::glass_edge_with_palette(
                painter, rect, 9.0, false, &palette,
            );
            return;
        }
        paint_button_with_palette(
            painter,
            ctx.border_box(),
            9.0,
            components::SettingsButton::Bare,
            false,
            Interaction {
                hovered: ctx.is_hovered() || self.highlighted,
                active: ctx.is_active(),
                ..Default::default()
            },
            &palette,
        );
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
        ChildrenIds::from_slice(&[self.artwork.id(), self.label.id(), self.check.id()])
    }
    fn propagates_pointer_interaction(&self) -> bool {
        false
    }
}
