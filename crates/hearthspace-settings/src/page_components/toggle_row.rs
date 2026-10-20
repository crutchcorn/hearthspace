//! A whole-card switch: one native Switch owns the label, track and focus stop.
use hearthspace_ui_components::{self as components, settings_material as material};
use xilem::masonry::{
    accesskit::Node,
    core::{
        ChildrenIds, LayoutCtx, MeasureCtx, MutateCtx, NewWidget, PaintCtx, RegisterCtx, WidgetPod,
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size},
    layout::{AsUnit, LayoutSize, LenReq, Length, SizeDef},
    parley::{
        FontFamily, FontFamilyName, StyleProperty,
        style::{FontWeight, LineHeight},
    },
    properties::LineBreaking,
    widgets::Label,
};

use super::toggle::Toggle;
use crate::state::SettingsApp;

pub(crate) fn toggle_row<F>(
    name: &'static str,
    description: Option<&'static str>,
    on: bool,
    disabled: bool,
    callback: F,
) -> Toggle<F>
where
    F: Fn(&mut SettingsApp, bool) + Send + Sync + 'static,
{
    super::toggle(name, on, disabled, callback).with_row(description)
}

const INSET: f64 = 13.0; // 12px padding and the one-pixel border.
const TRACK: f64 = 40.0;
const GAP: f64 = 16.0;

pub(super) struct RowContent {
    title: WidgetPod<Label>,
    detail: WidgetPod<Label>,
    description: Option<&'static str>,
}

fn label(text: &'static str, size: f32) -> Label {
    Label::new(text)
        .with_style(StyleProperty::FontSize(size))
        .with_style(FontFamily::Single(FontFamilyName::Named(
            crate::typography::FONT.into(),
        )))
}

impl RowContent {
    pub fn new(name: &'static str, description: Option<&'static str>) -> Self {
        Self {
            title: NewWidget::new(
                label(name, 11.0).with_style(StyleProperty::FontWeight(FontWeight::MEDIUM)),
            )
            .with_props(LineBreaking::WordWrap)
            .to_pod(),
            detail: NewWidget::new(
                label(description.unwrap_or(""), 10.0)
                    .with_style(StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.5))),
            )
            .with_props(LineBreaking::WordWrap)
            .with_class("settings-muted")
            .to_pod(),
            description,
        }
    }

    pub fn set_text(
        &mut self,
        ctx: &mut MutateCtx<'_>,
        name: &'static str,
        description: Option<&'static str>,
    ) {
        Label::set_text(&mut ctx.get_mut(&mut self.title), name);
        Label::set_text(
            &mut ctx.get_mut(&mut self.detail),
            description.unwrap_or(""),
        );
        self.description = description;
        ctx.request_layout();
    }

    pub fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.title);
        ctx.register_child(&mut self.detail);
    }

    pub fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        axis: Axis,
        request: LenReq,
        cross: Option<Length>,
    ) -> Length {
        let reserved = (INSET * 2.0 + TRACK + GAP).px();
        if axis == Axis::Horizontal {
            let available = match request {
                LenReq::FitContent(width) => LenReq::FitContent(width.saturating_sub(reserved)),
                other => other,
            };
            let title = ctx.compute_length(
                &mut self.title,
                available.into(),
                LayoutSize::NONE,
                axis,
                None,
            );
            let detail = if self.description.is_some() {
                ctx.compute_length(
                    &mut self.detail,
                    available.into(),
                    LayoutSize::NONE,
                    axis,
                    None,
                )
            } else {
                Length::ZERO
            };
            return title.max(detail).saturating_add(reserved);
        }
        let width = cross.map(|width| width.saturating_sub(reserved));
        let context = LayoutSize::maybe(Axis::Horizontal, width);
        let title = ctx.compute_length(
            &mut self.title,
            LenReq::MaxContent.into(),
            context,
            axis,
            width,
        );
        let detail = if self.description.is_some() {
            ctx.compute_length(
                &mut self.detail,
                LenReq::MaxContent.into(),
                context,
                axis,
                width,
            )
            .saturating_add(6.0.px())
        } else {
            Length::ZERO
        };
        title
            .saturating_add(detail)
            .saturating_add((INSET * 2.0).px())
            .max(68.0.px())
    }

    pub fn layout(&mut self, ctx: &mut LayoutCtx<'_>, size: Size) {
        let available = Size::new(
            (size.width - INSET * 2.0 - TRACK - GAP).max(0.0),
            size.height,
        );
        let title = ctx.compute_size(&mut self.title, SizeDef::fit(available), available.into());
        ctx.set_stashed(&mut self.detail, self.description.is_none());
        let detail = if self.description.is_some() {
            ctx.compute_size(&mut self.detail, SizeDef::fit(available), available.into())
        } else {
            Size::ZERO
        };
        let gap = if self.description.is_some() { 6.0 } else { 0.0 };
        let top = ((size.height - title.height - detail.height - gap) / 2.0).max(0.0);
        ctx.run_layout(&mut self.title, Size::new(available.width, title.height));
        ctx.place_child(&mut self.title, Point::new(INSET, top));
        if self.description.is_some() {
            ctx.run_layout(&mut self.detail, Size::new(available.width, detail.height));
            ctx.place_child(
                &mut self.detail,
                Point::new(INSET, top + title.height + gap),
            );
        }
    }

    pub fn paint(palette: components::Palette, ctx: &PaintCtx<'_>, painter: &mut Painter<'_>) {
        material::paint_button_with_palette(
            painter,
            ctx.border_box(),
            components::RADIUS_TILE,
            components::SettingsButton::Control,
            false,
            material::Interaction {
                hovered: ctx.is_hovered(),
                active: ctx.is_active(),
                // The reference focuses the switch track, not the whole card.
                focused: false,
                disabled: ctx.is_disabled(),
            },
            &palette,
        );
    }

    pub fn accessibility(&self, node: &mut Node) {
        if let Some(description) = self.description {
            node.set_description(description);
        }
    }

    pub fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.title.id(), self.detail.id()])
    }
}

#[cfg(test)]
mod tests;
