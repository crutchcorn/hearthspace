//! Native Button behavior with the confirmation's scoped red destructive style.
use hearthspace_ui_components::{self as components, settings_material as material};
use std::{cell::Cell, rc::Rc};
use xilem::{
    Color,
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
            PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
            UpdateCtx, Widget,
        },
        imaging::Painter,
        kurbo::{Axis, Size, Stroke},
        layout::{AsUnit, LenReq, Length},
        properties::{BorderColor, BorderWidth, ContentColor, Padding},
        widgets::{Button, ButtonPress},
    },
};

pub(super) struct ConfirmationButton {
    inner: Button,
    destructive: bool,
    keyboard: Rc<Cell<bool>>,
}

impl ConfirmationButton {
    pub fn new(name: &'static str, destructive: bool, keyboard: Rc<Cell<bool>>) -> NewWidget<Self> {
        let label = NewWidget::new(super::panel::label(name, 11.0));
        let label = if destructive {
            label.with_props(ContentColor::new(Color::WHITE))
        } else {
            label
        };
        NewWidget::new(Self {
            inner: Button::new(label),
            destructive,
            keyboard,
        })
        .with_props((
            Padding::from_vh(7.0.px(), 12.0.px()),
            BorderWidth { width: 1.0.px() },
            BorderColor {
                color: Color::TRANSPARENT,
            },
        ))
    }
}

impl Widget for ConfirmationButton {
    type Action = ButtonPress;
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.inner.on_pointer_event(ctx, props, event);
        ctx.request_paint_only();
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        self.inner.on_text_event(ctx, props, event);
        ctx.request_paint_only();
    }
    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        self.inner.on_access_event(ctx, props, event);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, props: &mut PropertiesMut<'_>, event: &Update) {
        self.inner.update(ctx, props, event);
        ctx.request_paint_only();
    }
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        self.inner.register_children(ctx);
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        props: &PropertiesRef<'_>,
        axis: Axis,
        request: LenReq,
        cross: Option<Length>,
    ) -> Length {
        self.inner.measure(ctx, props, axis, request, cross)
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, props: &PropertiesRef<'_>, size: Size) {
        self.inner.layout(ctx, props, size);
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
        let rect = ctx.border_box();
        let focused = ctx.is_focus_target() && self.keyboard.get();
        if self.destructive {
            let color = if ctx.is_hovered() {
                Color::from_rgb8(0xa6, 0x30, 0x2e)
            } else {
                Color::from_rgb8(0xbc, 0x3b, 0x39)
            };
            painter
                .fill(rect.to_rounded_rect(components::RADIUS_CONTROL), color)
                .draw();
            material::glass_edge_with_palette(
                painter,
                rect,
                components::RADIUS_CONTROL,
                ctx.is_hovered(),
                &palette,
            );
            if focused {
                painter
                    .stroke(
                        rect.inflate(3.0, 3.0)
                            .to_rounded_rect(components::RADIUS_CONTROL + 3.0),
                        &Stroke::new(2.0),
                        Color::from_rgb8(0xbc, 0x3b, 0x39),
                    )
                    .draw();
            }
        } else {
            material::paint_button_with_palette(
                painter,
                rect,
                components::RADIUS_CONTROL,
                components::SettingsButton::Control,
                false,
                material::Interaction {
                    hovered: ctx.is_hovered(),
                    active: ctx.is_active(),
                    focused,
                    disabled: ctx.is_disabled(),
                },
                &palette,
            );
        }
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
    }
    fn children_ids(&self) -> ChildrenIds {
        self.inner.children_ids()
    }
    fn propagates_pointer_interaction(&self) -> bool {
        false
    }
    fn accepts_focus(&self) -> bool {
        true
    }
}
