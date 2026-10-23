use super::button::ConfirmationButton;
use hearthspace_ui_components::{self as components, settings_material as material};
use std::{cell::Cell, rc::Rc};
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
        PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget, WidgetId, WidgetMut,
        WidgetPod,
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size, Stroke},
    layout::{AsUnit, LayoutSize, LenReq, Length, SizeDef},
    parley::{FontFamily, FontFamilyName, StyleProperty, style::LineHeight},
    properties::LineBreaking,
    widgets::Label,
};

const INSET: f64 = 25.0;
const BUTTON_HEIGHT: f64 = 40.0;
const ACTION_GAP: f64 = 8.0;
const DESCRIPTION: &str = "Turning off AI deletes the local model and its download. You’ll need to download it again to turn AI back on.";

#[derive(Debug)]
pub(super) struct FocusKeep;

pub(super) fn label(text: &'static str, size: f32) -> Label {
    Label::new(text)
        .with_style(StyleProperty::FontSize(size))
        .with_style(FontFamily::Single(FontFamilyName::Named(
            crate::typography::FONT.into(),
        )))
}

pub(super) struct ConfirmationPanel {
    title: WidgetPod<Label>,
    description: WidgetPod<Label>,
    keep: WidgetPod<ConfirmationButton>,
    delete: WidgetPod<ConfirmationButton>,
}

impl ConfirmationPanel {
    pub fn new(keyboard: Rc<Cell<bool>>) -> Self {
        Self {
            title: NewWidget::new(
                label("Turn off AI?", 20.0)
                    .with_style(StyleProperty::LetterSpacing(-0.4))
                    .with_style(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
                        1.15,
                    ))),
            )
            .to_pod(),
            description: NewWidget::new(
                label(DESCRIPTION, 12.0)
                    .with_style(StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.6))),
            )
            .with_props(LineBreaking::WordWrap)
            .with_class("settings-muted")
            .to_pod(),
            keep: ConfirmationButton::new("Keep AI", false, keyboard.clone()).to_pod(),
            delete: ConfirmationButton::new("Turn off and delete", true, keyboard).to_pod(),
        }
    }

    pub fn actions(&self) -> [WidgetId; 2] {
        [self.keep.id(), self.delete.id()]
    }

    pub fn repaint_actions(this: &mut WidgetMut<'_, Self>) {
        this.ctx
            .get_mut(&mut this.widget.keep)
            .ctx
            .request_paint_only();
        this.ctx
            .get_mut(&mut this.widget.delete)
            .ctx
            .request_paint_only();
    }
}

impl Widget for ConfirmationPanel {
    type Action = NoAction;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.title);
        ctx.register_child(&mut self.description);
        ctx.register_child(&mut self.keep);
        ctx.register_child(&mut self.delete);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded) {
            ctx.mutate_child_later(&mut self.keep, |mut keep| {
                keep.ctx.submit_untyped_action(Box::new(FocusKeep));
            });
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
        if axis == Axis::Horizontal {
            return match request {
                LenReq::MinContent => 0.0.px(),
                LenReq::MaxContent => 380.0.px(),
                LenReq::FitContent(available) => available.min(380.0.px()),
            };
        }
        let width = cross
            .unwrap_or(380.0.px())
            .saturating_sub((INSET * 2.0).px());
        let context = LayoutSize::maybe(Axis::Horizontal, Some(width));
        let title = ctx.compute_length(
            &mut self.title,
            LenReq::MaxContent.into(),
            context,
            axis,
            Some(width),
        );
        let body = ctx.compute_length(
            &mut self.description,
            LenReq::MaxContent.into(),
            context,
            axis,
            Some(width),
        );
        let buttons = if width.get() < 240.0 {
            BUTTON_HEIGHT * 2.0 + ACTION_GAP
        } else {
            BUTTON_HEIGHT
        };
        (title.get() + body.get() + INSET * 2.0 + 12.0 + 24.0 + buttons).px()
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        let inset = INSET.min(size.width / 4.0);
        let width = (size.width - inset * 2.0).max(0.0);
        let available = Size::new(width, size.height);
        let title = ctx.compute_size(&mut self.title, SizeDef::fit(available), available.into());
        let body = ctx.compute_size(
            &mut self.description,
            SizeDef::fit(available),
            available.into(),
        );
        ctx.run_layout(&mut self.title, Size::new(width, title.height));
        ctx.place_child(&mut self.title, Point::new(inset, INSET));
        ctx.run_layout(&mut self.description, Size::new(width, body.height));
        ctx.place_child(
            &mut self.description,
            Point::new(inset, INSET + title.height + 12.0),
        );
        let keep = ctx.compute_size(&mut self.keep, SizeDef::fit(available), available.into());
        let delete = ctx.compute_size(&mut self.delete, SizeDef::fit(available), available.into());
        let y = INSET + title.height + 12.0 + body.height + 24.0;
        if width < 240.0 {
            ctx.run_layout(&mut self.keep, Size::new(width, BUTTON_HEIGHT));
            ctx.place_child(&mut self.keep, Point::new(inset, y));
            ctx.run_layout(&mut self.delete, Size::new(width, BUTTON_HEIGHT));
            ctx.place_child(
                &mut self.delete,
                Point::new(inset, y + BUTTON_HEIGHT + ACTION_GAP),
            );
        } else {
            ctx.run_layout(&mut self.keep, Size::new(keep.width, BUTTON_HEIGHT));
            ctx.place_child(
                &mut self.keep,
                Point::new(
                    size.width - inset - keep.width - delete.width - ACTION_GAP,
                    y,
                ),
            );
            ctx.run_layout(&mut self.delete, Size::new(delete.width, BUTTON_HEIGHT));
            ctx.place_child(
                &mut self.delete,
                Point::new(size.width - inset - delete.width, y),
            );
        }
        ctx.set_paint_insets(40.0);
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
        material::shadow(
            painter,
            rect,
            20.0,
            palette.accent.with_alpha(0.18),
            (0.0, 28.0),
            88.0,
            -18.0,
        );
        material::shadow(
            painter,
            rect,
            20.0,
            palette.glow_shadow,
            (0.0, 8.0),
            32.0,
            -12.0,
        );
        // This native layer cannot backdrop-filter its already-painted sibling.
        // An opaque warm backing keeps the page text from showing through it.
        painter
            .fill(rect.to_rounded_rect(20.0), palette.popup_backing)
            .draw();
        material::edge_light_with_palette(painter, rect, 20.0, &palette);
        material::glass_edge_with_palette(painter, rect, 20.0, true, &palette);
        painter
            .stroke(
                rect.inset(-0.5).to_rounded_rect(19.5),
                &Stroke::new(1.0),
                palette.border,
            )
            .draw();
    }
    fn accessibility_role(&self) -> Role {
        Role::Dialog
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label("Turn off AI?");
        node.set_description(DESCRIPTION);
        node.set_modal();
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[
            self.title.id(),
            self.description.id(),
            self.keep.id(),
            self.delete.id(),
        ])
    }
}
