use hearthspace_ui_components::{
    self as components,
    icons::{Icon, IconWidget, icon},
};
use masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesMut,
        PropertiesRef, PropertySet, RegisterCtx, Update, UpdateCtx, Widget, WidgetPod,
    },
    imaging::{ClipRef, Painter},
    kurbo::{Axis, Point, Rect, Size},
    layout::{AsUnit, LenReq, Length, SizeDef},
    parley::{
        FontFamilyName, StyleProperty,
        style::{FontFamily, FontWeight},
    },
    properties::{ContentColor, LineBreaking},
    widgets::Label,
};
use xilem::Color;

use super::{ACTIONS_WIDTH, TitlebarState, paint};
use crate::{
    compositor::windows::{TitlebarButton, titlebar_button_rect},
    config::TITLE_BAR_HEIGHT,
};

/// A stable foreground strip. Window geometry never enters its rasterization.
pub(super) struct Strip {
    width: f64,
    content: Content,
    pub natural_width: f64,
}

enum Content {
    Text(WidgetPod<Label>),
    App(WidgetPod<IconWidget>),
    Actions {
        state: TitlebarState,
        maximize: WidgetPod<IconWidget>,
        close: WidgetPod<IconWidget>,
    },
}

impl Strip {
    pub fn text(text: String, width: f64, color: Color) -> Self {
        let label = Label::new(text)
            .with_style(StyleProperty::FontFamily(FontFamily::Single(
                FontFamilyName::Named("Liberation Sans".into()),
            )))
            .with_style(StyleProperty::FontSize(12.0))
            .with_style(StyleProperty::FontWeight(FontWeight::new(550.0)))
            .with_style(StyleProperty::LetterSpacing(-0.15))
            .prepare()
            .with_props(
                PropertySet::new()
                    .with(ContentColor::new(color))
                    .with(LineBreaking::Clip),
            )
            .to_pod();
        Self {
            width,
            content: Content::Text(label),
            natural_width: width,
        }
    }

    pub fn app(state: &TitlebarState) -> Self {
        let color = if state.active {
            components::settings_material::ACCENT_DECORATIVE
        } else {
            paint::muted(state.photo)
        };
        Self {
            width: 44.0,
            content: Content::App(WidgetPod::new(
                icon(app_icon(state.app_id.as_deref()), 15.0)
                    .color(color)
                    .into_widget(),
            )),
            natural_width: 44.0,
        }
    }

    pub fn actions(state: &TitlebarState) -> Self {
        let maximize = icon(
            if state.maximized {
                Icon::Minimize
            } else {
                Icon::Maximize
            },
            12.0,
        )
        .color(paint::action_color(state, TitlebarButton::Maximize))
        .into_widget();
        let close = icon(Icon::X, 15.0)
            .color(paint::action_color(state, TitlebarButton::Close))
            .into_widget();
        Self {
            width: f64::from(ACTIONS_WIDTH),
            content: Content::Actions {
                state: state.clone(),
                maximize: WidgetPod::new(maximize),
                close: WidgetPod::new(close),
            },
            natural_width: f64::from(ACTIONS_WIDTH),
        }
    }
}

impl Widget for Strip {
    type Action = NoAction;

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded) {
            super::fonts::install(ctx.text_contexts().0);
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        match &mut self.content {
            Content::Text(child) => ctx.register_child(child),
            Content::App(child) => ctx.register_child(child),
            Content::Actions {
                maximize, close, ..
            } => {
                ctx.register_child(maximize);
                ctx.register_child(close);
            }
        }
    }

    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        _: Option<Length>,
    ) -> Length {
        match axis {
            Axis::Horizontal => self.width.px(),
            Axis::Vertical => TITLE_BAR_HEIGHT.px(),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        match &mut self.content {
            Content::Text(child) => {
                let measured =
                    ctx.compute_size(child, SizeDef::MAX, Size::new(self.width, 43.0).into());
                self.natural_width = measured.width;
                ctx.run_layout(child, Size::new(size.width, measured.height));
                ctx.place_child(child, Point::new(0.0, (43.0 - measured.height) / 2.0));
            }
            Content::App(child) => {
                ctx.run_layout(child, Size::new(15.0, 15.0));
                ctx.place_child(child, Point::new(13.0, 14.0));
            }
            Content::Actions {
                maximize, close, ..
            } => {
                for (button, glyph, child) in [
                    (TitlebarButton::Maximize, 12.0, maximize),
                    (TitlebarButton::Close, 15.0, close),
                ] {
                    let rect = action_bounds(ACTIONS_WIDTH, button);
                    ctx.run_layout(child, Size::new(glyph, glyph));
                    ctx.place_child(
                        child,
                        Point::new(
                            rect.x0 + (rect.width() - glyph) / 2.0,
                            rect.y0 + (rect.height() - glyph) / 2.0,
                        ),
                    );
                }
            }
        }
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        painter.push_clip(ClipRef::fill(ctx.border_box()));
        if let Content::Actions { state, .. } = &self.content {
            for button in [TitlebarButton::Maximize, TitlebarButton::Close] {
                paint::action(painter, action_bounds(ACTIONS_WIDTH, button), state, button);
            }
        }
    }

    fn post_paint(
        &mut self,
        _: &mut PaintCtx<'_>,
        _: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        painter.pop_clip();
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        match &self.content {
            Content::Text(child) => ChildrenIds::from_slice(&[child.id()]),
            Content::App(child) => ChildrenIds::from_slice(&[child.id()]),
            Content::Actions {
                maximize, close, ..
            } => ChildrenIds::from_slice(&[maximize.id(), close.id()]),
        }
    }
}

fn app_icon(app_id: Option<&str>) -> Icon {
    match app_id {
        Some("dev.hearthspace.Settings" | "dev.hearthspace.settings" | "hearthspace-settings") => {
            Icon::AppSettings
        }
        Some("foot" | "footclient" | "org.codeberg.dnkl.foot") => Icon::Terminal,
        _ => Icon::Square,
    }
}

pub(super) fn action_bounds(width: i32, button: TitlebarButton) -> Rect {
    let rect = titlebar_button_rect(width, button);
    Rect::new(
        f64::from(rect.loc.x),
        f64::from(rect.loc.y) - 0.5,
        f64::from(rect.loc.x + rect.size.w),
        f64::from(rect.loc.y + rect.size.h) - 0.5,
    )
}
