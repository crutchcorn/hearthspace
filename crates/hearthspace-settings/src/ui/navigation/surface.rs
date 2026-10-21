use hearthspace_ui_components as components;
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
        PropertiesRef, RegisterCtx, Widget, WidgetMut, WidgetPod,
    },
    imaging::Painter,
    kurbo::{Axis, Point, Rect, Size},
    layout::{LenReq, Length},
};

pub(crate) struct NavigationMenu {
    pub(super) inner: WidgetPod<dyn Widget>,
    pub(super) compact: bool,
}
impl NavigationMenu {
    pub(super) fn new(inner: NewWidget<impl Widget + ?Sized>, compact: bool) -> Self {
        Self {
            inner: inner.erased().to_pod(),
            compact,
        }
    }
    pub(super) fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.inner)
    }
}
impl Widget for NavigationMenu {
    type Action = NoAction;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.inner);
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        _: Option<Length>,
    ) -> Length {
        ctx.context_size().length(axis).unwrap_or(Length::ZERO)
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        // Padding lives inside the scroll area, so the final row remains reachable
        // when a short window limits the overlay's natural 282px height.
        ctx.run_layout(
            &mut self.inner,
            Size::new(
                size.width - if self.compact { 0.0 } else { 1.0 },
                (size.height - if self.compact { 1.0 } else { 0.0 }).max(0.0),
            ),
        );
        ctx.place_child(&mut self.inner, Point::ORIGIN);
        ctx.set_paint_insets(if self.compact { 40.0 } else { 0.0 });
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
        if self.compact {
            components::settings_material::shadow(
                painter,
                rect,
                0.0,
                palette.panel_shadow,
                (0.0, 24.0),
                72.0,
                -22.0,
            );
            components::settings_material::shadow(
                painter,
                rect,
                0.0,
                palette.panel_accent_shadow,
                (0.0, 6.0),
                24.0,
                -8.0,
            );
            painter.fill(rect, palette.chrome_raised).draw();
            components::settings_material::edge_light_with_palette(painter, rect, 0.0, &palette);
            components::settings_material::glass_edge_with_palette(
                painter, rect, 0.0, false, &palette,
            );
            painter
                .fill(
                    Rect::new(0.0, rect.y1 - 1.0, rect.x1, rect.y1),
                    palette.border,
                )
                .draw();
        } else {
            painter.fill(rect, palette.surface_subtle).draw();
            painter
                .fill(
                    Rect::new(rect.x1 - 1.0, 0.0, rect.x1, rect.y1),
                    palette.border,
                )
                .draw();
        }
    }
    fn accessibility_role(&self) -> Role {
        Role::Navigation
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label("Settings categories");
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.inner.id()])
    }
}
