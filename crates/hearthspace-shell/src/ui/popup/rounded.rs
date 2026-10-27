//! Clip full-bleed popup contents without changing their layout or semantics.
use super::ShellState;
use hearthspace_ui_components::{
    Palette,
    settings_material::{edge_light_with_palette, glass_edge_with_palette},
};
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef,
            RegisterCtx, UpdateCtx, UsesProperty, Widget, WidgetId, WidgetMut, WidgetPod,
        },
        imaging::{ClipRef, Painter},
        kurbo::{Axis, Point, Size, Stroke},
        layout::{LenReq, Length},
    },
};

pub(super) fn clip<V: WidgetView<ShellState>>(inner: V, glass: bool) -> RoundedView<V> {
    RoundedView { inner, glass }
}

pub(super) struct RoundedView<V> {
    inner: V,
    glass: bool,
}

const CONTENT: ViewId = ViewId::new(0);
impl<V> ViewMarker for RoundedView<V> {}
impl<V: WidgetView<ShellState>> View<ShellState, (), ViewCtx> for RoundedView<V> {
    type Element = Pod<RoundedFrame>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let (inner, view_state) = ctx.with_id(CONTENT, |ctx| self.inner.build(ctx, state));
        (
            ctx.create_pod(RoundedFrame {
                inner: inner.new_widget.erased().to_pod(),
                glass: self.glass,
            }),
            view_state,
        )
    }

    fn rebuild(
        &self,
        previous: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) {
        if self.glass != previous.glass {
            element.widget.glass = self.glass;
            element.ctx.request_paint_only();
        }
        ctx.with_id(CONTENT, |ctx| {
            self.inner.rebuild(
                &previous.inner,
                view_state,
                ctx,
                RoundedFrame::child_mut(&mut element).downcast(),
                state,
            )
        });
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(CONTENT, |ctx| {
            self.inner.teardown(
                view_state,
                ctx,
                RoundedFrame::child_mut(&mut element).downcast(),
            )
        });
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        match message.take_first() {
            Some(CONTENT) => self.inner.message(
                view_state,
                message,
                RoundedFrame::child_mut(&mut element).downcast(),
                state,
            ),
            _ => MessageResult::Stale,
        }
    }
}

pub(in crate::ui) struct RoundedFrame {
    inner: WidgetPod<dyn Widget>,
    glass: bool,
}
impl UsesProperty<Palette> for RoundedFrame {}
impl RoundedFrame {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.inner)
    }
}
impl Widget for RoundedFrame {
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
        cross: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.inner, axis, cross)
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.inner, size);
        ctx.place_child(&mut self.inner, Point::ORIGIN);
        ctx.derive_baselines(&self.inner);
    }
    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: std::any::TypeId) {
        Palette::prop_changed(ctx, property_type);
    }
    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        // LayoutCtx clips only rectangles in the pinned Masonry version.
        // Its paint and post_paint passes bracket all descendant painting.
        painter.push_clip(ClipRef::fill(ctx.border_box().to_rounded_rect(20.0)));
        if self.glass {
            // Blur and unclipped outer shadows are supplied by the compositor.
            let palette = props.get::<Palette>(ctx.property_cache());
            let rect = ctx.border_box();
            let shape = rect.inset(-0.5).to_rounded_rect(19.5);
            painter.fill(shape, palette.chrome).draw();
            edge_light_with_palette(painter, rect, 20.0, palette);
            glass_edge_with_palette(painter, rect, 20.0, false, palette);
            painter
                .stroke(shape, &Stroke::new(1.0), palette.chrome_border)
                .draw();
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
        ChildrenIds::from_slice(&[self.inner.id()])
    }
    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("RoundedPopup", id = id.trace())
    }
}
