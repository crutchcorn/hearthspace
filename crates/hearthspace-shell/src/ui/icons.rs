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
        kurbo::{Axis, BezPath, Circle, Line, Rect, Size, Stroke},
        layout::{AsUnit, LenReq, Length},
    },
    style::Style,
    view::{button, sized_box},
};

use super::state::ShellState;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Icon {
    Map,
    SelectGroup,
    Search,
    Draw,
    Settings,
    Bell { active: bool },
    Power,
    Controls,
    Brightness,
    Volume { muted: bool },
    ZoomIn,
    ZoomOut,
    Close,
    Battery { percentage: u8, charging: bool },
}

pub(super) fn glyph(icon: Icon, name: String) -> impl WidgetView<ShellState> + use<> {
    sized_box(IconView {
        icon,
        name,
        selected: false,
    })
    .fixed_width(20.0.px())
    .fixed_height(20.0.px())
}

pub(super) fn icon_button<F: Fn(&mut ShellState) + Send + Sync + 'static>(
    icon: Icon,
    name: &str,
    selected: bool,
    action: F,
) -> impl WidgetView<ShellState> + use<F> {
    let glyph = IconView {
        icon,
        name: name.to_string(),
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
    name: String,
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
                name: self.name.clone(),
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
            element.widget.name = self.name.clone();
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
    name: String,
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
            Icon::SelectGroup => {
                // Four selection corners around overlapping window outlines.
                for (x, y, dx, dy) in [
                    (1.0, 1.0, 4.0, 4.0),
                    (19.0, 1.0, -4.0, 4.0),
                    (1.0, 19.0, 4.0, -4.0),
                    (19.0, 19.0, -4.0, -4.0),
                ] {
                    let mut path = BezPath::new();
                    path.move_to((x + dx, y));
                    path.line_to((x, y));
                    path.line_to((x, y + dy));
                    painter.stroke(&path, &stroke, color).draw();
                }
                painter
                    .stroke(Rect::new(5.0, 5.0, 12.0, 12.0), &stroke, color)
                    .draw();
                painter
                    .fill(Rect::new(8.0, 8.0, 15.0, 15.0), components::SURFACE)
                    .draw();
                painter
                    .stroke(Rect::new(8.0, 8.0, 15.0, 15.0), &stroke, color)
                    .draw();
            }
            Icon::Draw => {
                let mut path = BezPath::new();
                path.move_to((3.0, 13.0));
                path.line_to((13.0, 3.0));
                path.quad_to((14.5, 1.5), (16.0, 3.0));
                path.line_to((17.0, 4.0));
                path.quad_to((18.5, 5.5), (17.0, 7.0));
                path.line_to((7.0, 17.0));
                path.line_to((2.0, 18.0));
                path.close_path();
                painter.stroke(&path, &stroke, color).draw();
                painter
                    .stroke(Line::new((11.5, 4.5), (15.5, 8.5)), &stroke, color)
                    .draw();
            }
            Icon::Settings => {
                let mut gear = BezPath::new();
                for index in 0..32 {
                    let angle = f64::from(index) * std::f64::consts::TAU / 32.0;
                    let radius = if index % 4 < 2 { 8.5 } else { 6.5 };
                    let point = (10.0 + radius * angle.cos(), 10.0 + radius * angle.sin());
                    if index == 0 {
                        gear.move_to(point);
                    } else {
                        gear.line_to(point);
                    }
                }
                gear.close_path();
                painter.stroke(&gear, &stroke, color).draw();
                painter
                    .stroke(Circle::new((10.0, 10.0), 2.8), &stroke, color)
                    .draw();
            }
            Icon::Controls => {
                for (y, knob) in [(4.0, 7.0), (10.0, 13.0), (16.0, 7.0)] {
                    painter
                        .stroke(Line::new((2.0, y), (18.0, y)), &stroke, color)
                        .draw();
                    painter
                        .fill(Circle::new((knob, y), 2.5), components::SURFACE)
                        .draw();
                    painter
                        .stroke(Circle::new((knob, y), 2.5), &stroke, color)
                        .draw();
                }
            }
            Icon::Brightness => {
                painter
                    .stroke(Circle::new((10.0, 10.0), 4.0), &stroke, color)
                    .draw();
                for n in 0..8 {
                    let angle = f64::from(n) * std::f64::consts::FRAC_PI_4;
                    let (sin, cos) = angle.sin_cos();
                    painter
                        .stroke(
                            Line::new(
                                (10.0 + 7.0 * cos, 10.0 + 7.0 * sin),
                                (10.0 + 9.0 * cos, 10.0 + 9.0 * sin),
                            ),
                            &stroke,
                            color,
                        )
                        .draw();
                }
            }
            Icon::Volume { muted } => {
                let mut path = BezPath::new();
                path.move_to((2.0, 7.0));
                path.line_to((6.0, 7.0));
                path.line_to((10.0, 3.0));
                path.line_to((10.0, 17.0));
                path.line_to((6.0, 13.0));
                path.line_to((2.0, 13.0));
                path.close_path();
                painter.stroke(&path, &stroke, color).draw();
                if muted {
                    painter
                        .stroke(Line::new((14.0, 7.0), (19.0, 13.0)), &stroke, color)
                        .draw();
                    painter
                        .stroke(Line::new((19.0, 7.0), (14.0, 13.0)), &stroke, color)
                        .draw();
                } else {
                    for (x, y) in [(13.0, 6.0), (16.0, 3.0)] {
                        let mut wave = BezPath::new();
                        wave.move_to((x, y));
                        wave.curve_to((x + 4.0, 10.0), (x + 4.0, 10.0), (x, 20.0 - y));
                        painter.stroke(&wave, &stroke, color).draw();
                    }
                }
            }
            Icon::Battery {
                percentage,
                charging,
            } => {
                painter
                    .stroke(Rect::new(1.0, 5.0, 17.0, 15.0), &stroke, color)
                    .draw();
                painter.fill(Rect::new(18.0, 8.0, 20.0, 12.0), color).draw();
                if charging {
                    let mut bolt = BezPath::new();
                    bolt.move_to((10.0, 6.5));
                    bolt.line_to((6.0, 10.5));
                    bolt.line_to((9.0, 10.5));
                    bolt.line_to((8.0, 13.5));
                    bolt.line_to((12.0, 9.5));
                    bolt.line_to((9.0, 9.5));
                    bolt.close_path();
                    painter.fill(&bolt, color).draw();
                } else if percentage > 0 {
                    let width = 12.0 * f64::from(percentage.min(100)) / 100.0;
                    painter
                        .fill(Rect::new(3.0, 7.0, 3.0 + width, 13.0), color)
                        .draw();
                }
            }
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
            Icon::Search | Icon::ZoomIn | Icon::ZoomOut => {
                painter
                    .stroke(Circle::new((8.5, 8.5), 6.0), &stroke, color)
                    .draw();
                painter
                    .stroke(Line::new((13.0, 13.0), (18.0, 18.0)), &stroke, color)
                    .draw();
                if !matches!(self.icon, Icon::Search) {
                    painter
                        .stroke(Line::new((5.5, 8.5), (11.5, 8.5)), &stroke, color)
                        .draw();
                }
                if matches!(self.icon, Icon::ZoomIn) {
                    painter
                        .stroke(Line::new((8.5, 5.5), (8.5, 11.5)), &stroke, color)
                        .draw();
                }
            }
            Icon::Bell { active } => {
                let mut path = BezPath::new();
                path.move_to((3.0, 14.0));
                path.curve_to((5.0, 12.0), (4.5, 10.0), (4.5, 8.0));
                path.curve_to((4.5, 1.0), (15.5, 1.0), (15.5, 8.0));
                path.curve_to((15.5, 10.0), (15.0, 12.0), (17.0, 14.0));
                path.close_path();
                painter.stroke(&path, &stroke, color).draw();
                let mut clapper = BezPath::new();
                clapper.move_to((7.5, 16.0));
                clapper.curve_to((8.0, 19.0), (12.0, 19.0), (12.5, 16.0));
                painter.stroke(&clapper, &stroke, color).draw();
                if active {
                    painter
                        .fill(Circle::new((16.0, 4.0), 3.0), components::ACCENT)
                        .draw();
                }
            }
            Icon::Power => {
                let mut path = BezPath::new();
                path.move_to((5.0, 4.0));
                path.curve_to((-1.0, 9.0), (3.0, 18.0), (10.0, 18.0));
                path.curve_to((17.0, 18.0), (21.0, 9.0), (15.0, 4.0));
                painter.stroke(&path, &stroke, color).draw();
                painter
                    .stroke(Line::new((10.0, 1.5), (10.0, 10.0)), &stroke, color)
                    .draw();
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
        node.set_label(self.name.clone());
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("ShellIcon", id = id.trace())
    }
}
