use super::{
    FocusIndicator, Interaction, Material, paint_button_with_palette, paint_surface_with_palette,
};
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
        PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx,
        UsesProperty, Widget, WidgetMut, WidgetPod,
    },
    imaging::Painter,
    kurbo::{Axis, Point, Size},
    layout::{LayoutSize, LenReq, Length},
};

pub struct MaterialWidget {
    child: WidgetPod<dyn Widget>,
    pub(super) material: Material,
    child_disabled: bool,
    focus: FocusIndicator,
}

impl MaterialWidget {
    pub(super) fn new(
        child: NewWidget<impl Widget + ?Sized>,
        material: Material,
        disabled: bool,
    ) -> Self {
        Self {
            child: child.erased().to_pod(),
            material,
            child_disabled: disabled,
            focus: FocusIndicator::default(),
        }
    }

    pub(super) fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl UsesProperty<crate::Palette> for MaterialWidget {}

impl Widget for MaterialWidget {
    type Action = NoAction;
    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.focus.pointer(event);
        ctx.request_paint_only();
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        self.focus.text(event);
        ctx.request_paint_only();
    }
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        self.focus.update(event);
        if matches!(
            event,
            Update::ChildFocusChanged(_)
                | Update::ChildHoveredChanged(_)
                | Update::ChildActiveChanged(_)
                | Update::DisabledChanged(_)
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
        ctx.set_paint_insets(24.0);
    }
    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: std::any::TypeId) {
        crate::Palette::prop_changed(ctx, property_type);
    }
    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let palette = *props.get::<crate::Palette>(ctx.property_cache());
        match self.material {
            Material::Button(kind, selected) => paint_button_with_palette(
                painter,
                ctx.border_box(),
                self.material.radius(),
                kind,
                selected,
                Interaction {
                    hovered: ctx.has_hovered(),
                    active: ctx.has_active(),
                    focused: ctx.has_focus_target() && self.focus.visible(),
                    disabled: ctx.is_disabled() || self.child_disabled,
                },
                &palette,
            ),
            Material::Surface(kind) => paint_surface_with_palette(
                painter,
                ctx.border_box(),
                self.material.radius(),
                kind,
                &palette,
            ),
        }
    }
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}
