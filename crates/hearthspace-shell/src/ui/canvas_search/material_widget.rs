use super::Kind;
use hearthspace_ui_components::{
    Palette,
    settings_material::{edge_light_with_palette, glass_edge_with_palette},
};
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
        PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size, Stroke},
    layout::{LayoutSize, LenReq, Length},
};

pub(in crate::ui) struct PanelWidget {
    child: WidgetPod<dyn Widget>,
    pub(super) kind: Kind,
    pub(super) selected: bool,
    pub(super) hovered: bool,
    pub(super) palette: Palette,
}

impl PanelWidget {
    pub(super) fn new(
        child: NewWidget<impl Widget + ?Sized>,
        kind: Kind,
        selected: bool,
        hovered: bool,
        palette: Palette,
    ) -> Self {
        Self {
            child: child.erased().to_pod(),
            kind,
            selected,
            hovered,
            palette,
        }
    }
    pub(super) fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for PanelWidget {
    type Action = NoAction;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(
            event,
            Update::ChildHoveredChanged(_)
                | Update::ChildFocusChanged(_)
                | Update::ChildActiveChanged(_)
        ) {
            ctx.request_paint_only();
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
        ctx.compute_length(
            &mut self.child,
            request.into(),
            LayoutSize::maybe(axis.cross(), cross),
            axis,
            cross,
        )
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
        ctx.set_paint_insets(1.0);
    }
    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let palette = &self.palette;
        let active = self.selected || self.hovered || ctx.has_hovered() || ctx.has_focus_target();
        let rect = ctx.border_box();
        if self.kind == Kind::Choice {
            let shape = rect.to_rounded_rect(15.0);
            if self.selected || ctx.has_hovered() {
                painter
                    .fill(
                        shape,
                        if self.selected {
                            palette.accent_soft
                        } else {
                            palette.raised_button_hover
                        },
                    )
                    .draw();
            }
            if ctx.has_focus_target() {
                painter
                    .stroke(
                        rect.inset(-1.0).to_rounded_rect(14.0),
                        &Stroke::new(2.0),
                        palette.accent,
                    )
                    .draw();
            }
            return;
        }
        let shell = matches!(self.kind, Kind::Shell | Kind::Tooltip);
        let active = active && self.kind != Kind::Tooltip;
        let radius = if self.kind == Kind::Tooltip {
            10.0
        } else {
            20.0
        };
        let shape = rect.inset(-0.5).to_rounded_rect(radius - 0.5);
        let raised = shell || (self.kind == Kind::Card && active);
        painter
            .fill(
                shape,
                if shell && active {
                    palette.chrome_raised_hover
                } else if raised {
                    palette.chrome_raised
                } else {
                    palette.chrome
                },
            )
            .draw();
        if shell {
            // The same translucent raised fill and radial light as the reference.
            // Backdrop blur and unclipped outer shadows belong to the compositor.
            edge_light_with_palette(painter, rect, radius, palette);
        }
        glass_edge_with_palette(painter, rect, radius, active, palette);
        let selected_card = self.kind == Kind::Card && self.selected;
        painter
            .stroke(
                shape,
                &Stroke::new(if selected_card { 2.0 } else { 1.0 }),
                if selected_card {
                    palette.accent
                } else if (self.kind == Kind::Field || shell) && active {
                    palette.accent_border
                } else {
                    palette.chrome_border
                },
            )
            .draw();
    }
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}
