//! Small vector icons with accessible names inherited by their buttons.
use hearthspace_ui_components as components;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef,
            RegisterCtx, Widget, WidgetId,
        },
        imaging::Painter,
        kurbo::{Axis, BezPath, Circle, Line, Size, Stroke},
        layout::{AsUnit, LenReq, Length},
    },
    style::Style,
    view::{button, sized_box},
};

use super::state::ShellState;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Icon {
    Map,
    ZoomIn,
    ZoomOut,
    Close,
}

pub(super) fn icon_button<F: Fn(&mut ShellState) + Send + Sync + 'static>(
    icon: Icon,
    name: &'static str,
    selected: bool,
    action: F,
) -> impl WidgetView<ShellState> + use<F> {
    let glyph = IconView {
        icon,
        name,
        selected,
    };
    button(
        sized_box(glyph)
            .fixed_width(20.0.px())
            .fixed_height(20.0.px()),
        action,
    )
    .padding(7.0.px())
    .corner_radius(6.0.px())
    .background_color(if selected {
        components::ACCENT_SUBTLE
    } else {
        xilem::Color::TRANSPARENT
    })
}

// Image labels participate in AccessKit's button-name computation. Canvas alt
// text is a description and would leave these icon-only buttons unnamed.
struct IconView {
    icon: Icon,
    name: &'static str,
    selected: bool,
}
impl ViewMarker for IconView {}
impl View<ShellState, (), ViewCtx> for IconView {
    type Element = Pod<IconWidget>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut ShellState) -> (Self::Element, ()) {
        (
            ctx.create_pod(IconWidget {
                icon: self.icon,
                name: self.name,
                selected: self.selected,
            }),
            (),
        )
    }
    fn rebuild(
        &self,
        previous: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut ShellState,
    ) {
        if self.icon != previous.icon || self.selected != previous.selected {
            element.widget.icon = self.icon;
            element.widget.selected = self.selected;
            element.ctx.request_render();
        }
        if self.name != previous.name {
            element.widget.name = self.name;
            element.ctx.request_accessibility_update();
        }
    }
    fn teardown(&self, _: &mut (), _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}
    fn message(
        &self,
        _: &mut (),
        _: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        _: &mut ShellState,
    ) -> MessageResult<()> {
        MessageResult::Stale
    }
}

struct IconWidget {
    icon: Icon,
    name: &'static str,
    selected: bool,
}
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
        20.0.px()
    }
    fn layout(&mut self, _: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, _: Size) {}
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let stroke = Stroke::new(1.6);
        let color = if self.selected {
            components::ACCENT
        } else {
            components::TEXT
        };
        match self.icon {
            Icon::Map => {
                let mut path = BezPath::new();
                path.move_to((2.0, 4.5));
                path.line_to((7.0, 2.5));
                path.line_to((13.0, 4.5));
                path.line_to((18.0, 2.5));
                path.line_to((18.0, 15.5));
                path.line_to((13.0, 17.5));
                path.line_to((7.0, 15.5));
                path.line_to((2.0, 17.5));
                path.close_path();
                painter.stroke(&path, &stroke, color).draw();
                for (x, y) in [(7.0, 2.5), (13.0, 4.5)] {
                    painter
                        .stroke(Line::new((x, y), (x, y + 13.0)), &stroke, color)
                        .draw();
                }
            }
            Icon::ZoomIn | Icon::ZoomOut => {
                painter
                    .stroke(Circle::new((8.5, 8.5), 6.0), &stroke, color)
                    .draw();
                painter
                    .stroke(Line::new((13.0, 13.0), (18.0, 18.0)), &stroke, color)
                    .draw();
                painter
                    .stroke(Line::new((5.5, 8.5), (11.5, 8.5)), &stroke, color)
                    .draw();
                if matches!(self.icon, Icon::ZoomIn) {
                    painter
                        .stroke(Line::new((8.5, 5.5), (8.5, 11.5)), &stroke, color)
                        .draw();
                }
            }
            Icon::Close => {
                painter
                    .stroke(Line::new((5.0, 5.0), (15.0, 15.0)), &stroke, color)
                    .draw();
                painter
                    .stroke(Line::new((15.0, 5.0), (5.0, 15.0)), &stroke, color)
                    .draw();
            }
        }
    }
    fn accessibility_role(&self) -> Role {
        Role::Image
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.name);
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("MapIcon", id = id.trace())
    }
}
