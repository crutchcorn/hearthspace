//! Hearthspace's native vector icon set, ported from the UI experiment's SVGs.
//! Glyphs retain the 24×24 grid, square caps, miter joins, and optical strokes.
//! App artwork remains separate; these glyphs identify Hearthspace controls.
use xilem::{
    Color, Pod, ViewCtx,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef,
            RegisterCtx, UpdateCtx, UsesProperty, Widget,
        },
        imaging::Painter,
        kurbo::{Affine, Axis, Cap, Join, Size, Stroke},
        layout::{AsUnit, LenReq, Length},
        properties::ContentColor,
    },
};

mod geometry;
mod glyph;
mod kind;
use geometry::Geometry;
pub use glyph::IconGlyph;
pub use kind::Icon;
#[cfg(test)]
mod tests;

/// A decorative glyph. Use `.label(...)` when it supplies an icon-only button's name.
pub fn icon(kind: Icon, size: f64) -> IconView {
    IconView {
        kind,
        size: if size.is_finite() {
            size.max(0.0)
        } else {
            24.0
        },
        color: None,
        name: None,
        filled: false,
    }
}

#[derive(Clone, PartialEq)]
pub struct IconView {
    kind: Icon,
    size: f64,
    color: Option<Color>,
    name: Option<String>,
    filled: bool,
}

impl IconView {
    /// Use the same glyph in a Masonry widget tree, such as compositor chrome.
    pub fn into_widget(self) -> IconWidget {
        IconWidget {
            geometry: Geometry::new(self.kind, self.filled),
            view: self,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn label(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// App and document glyphs use their dedicated solid SVG geometry.
    /// Other glyphs retain their outline, matching the experiment's fallback.
    pub fn filled(mut self) -> Self {
        self.filled = true;
        self
    }
}

impl ViewMarker for IconView {}
impl<State: 'static, Action: 'static> View<State, Action, ViewCtx> for IconView {
    type Element = Pod<IconWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, ()) {
        (ctx.create_pod(self.clone().into_widget()), ())
    }

    fn rebuild(
        &self,
        previous: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if self.kind != previous.kind || self.filled != previous.filled {
            element.widget.geometry = Geometry::new(self.kind, self.filled);
        }
        if self != previous {
            element.widget.view = self.clone();
            element.ctx.request_paint_only();
        }
        if self.size != previous.size {
            element.ctx.request_layout();
        }
        if self.name != previous.name {
            element.ctx.request_accessibility_update();
        }
    }

    fn teardown(&self, _: &mut (), _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        _: &mut (),
        _: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        _: &mut State,
    ) -> MessageResult<Action> {
        MessageResult::Stale
    }
}

pub struct IconWidget {
    view: IconView,
    geometry: Geometry,
}

impl UsesProperty<ContentColor> for IconWidget {}

impl Widget for IconWidget {
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
        self.view.size.px()
    }

    fn layout(&mut self, _: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, _: Size) {}

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: std::any::TypeId) {
        if property_type == std::any::TypeId::of::<ContentColor>() {
            ctx.request_paint_only();
        }
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let color = self
            .view
            .color
            .unwrap_or_else(|| props.get::<ContentColor>(ctx.property_cache()).color);
        let bounds = ctx.content_box();
        let size = bounds.width().min(bounds.height());
        let transform = Affine::translate((
            (bounds.width() - size) / 2.0,
            (bounds.height() - size) / 2.0,
        )) * Affine::scale(size / 24.0);
        let stroke = Stroke::new(optical_stroke(size))
            .with_caps(Cap::Square)
            .with_join(Join::Miter)
            .with_miter_limit(2.0);
        for part in &self.geometry.0 {
            if let Some(rule) = part.fill {
                painter
                    .fill(&part.path, color)
                    .fill_rule(rule)
                    .transform(transform)
                    .draw();
            } else {
                painter
                    .stroke(&part.path, &stroke, color)
                    .transform(transform)
                    .draw();
            }
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Image
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        if let Some(name) = &self.view.name {
            // Labels, rather than descriptions, supply a parent button's name.
            node.set_label(name.clone());
        } else {
            node.set_hidden();
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

fn optical_stroke(size: f64) -> f64 {
    if size <= 16.0 {
        1.75
    } else if size >= 32.0 {
        1.25
    } else {
        1.5
    }
}
