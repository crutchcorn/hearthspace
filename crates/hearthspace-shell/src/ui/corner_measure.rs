//! Measure a system row's preferred width while still laying it out within its
//! allocated output width. A resize observer alone reports the clipped width.
use super::{ShellOutputInstance, ShellState};
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, PaintCtx, PropertiesRef, RegisterCtx,
            Widget, WidgetId, WidgetPod,
        },
        imaging::Painter,
        kurbo::{Axis, Point, Size},
        layout::{LenReq, Length, SizeDef},
    },
};

pub(super) fn content<V: WidgetView<ShellState>>(
    output: &ShellOutputInstance,
    hidden_width: f64,
    measure_width: bool,
    inner: V,
) -> MeasureView<V> {
    MeasureView {
        key: output.key().into(),
        hidden_width,
        measure_width,
        inner,
    }
}

pub(super) struct MeasureView<V> {
    key: String,
    hidden_width: f64,
    measure_width: bool,
    inner: V,
}
const CONTENT: ViewId = ViewId::new(0);
impl<V> ViewMarker for MeasureView<V> {}
impl<V: WidgetView<ShellState>> View<ShellState, (), ViewCtx> for MeasureView<V> {
    type Element = Pod<MeasureContent>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let (inner, view_state) = ctx.with_id(CONTENT, |ctx| self.inner.build(ctx, state));
        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(MeasureContent {
                    inner: inner.new_widget.erased().to_pod(),
                    hidden_width: self.hidden_width,
                    measure_width: self.measure_width,
                    width: None,
                })
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
        if self.hidden_width != previous.hidden_width {
            element.widget.hidden_width = self.hidden_width;
            element.ctx.request_layout();
        }
        if self.measure_width != previous.measure_width {
            element.widget.measure_width = self.measure_width;
            element.ctx.request_layout();
        }
        ctx.with_id(CONTENT, |ctx| {
            let xilem::masonry::core::WidgetMut {
                widget,
                ctx: child_ctx,
                ..
            } = &mut element;
            self.inner.rebuild(
                &previous.inner,
                view_state,
                ctx,
                child_ctx.get_mut(&mut widget.inner).downcast(),
                state,
            );
        });
    }
    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(CONTENT, |ctx| {
            let xilem::masonry::core::WidgetMut {
                widget,
                ctx: child_ctx,
                ..
            } = &mut element;
            self.inner.teardown(
                view_state,
                ctx,
                child_ctx.get_mut(&mut widget.inner).downcast(),
            );
        });
        ctx.teardown_action_source(element);
    }
    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        match message.take_first() {
            Some(CONTENT) => {
                let xilem::masonry::core::WidgetMut { widget, ctx, .. } = &mut element;
                self.inner.message(
                    view_state,
                    message,
                    ctx.get_mut(&mut widget.inner).downcast(),
                    state,
                )
            }
            None => {
                let Some(width) = message.take_message::<PreferredWidth>() else {
                    return MessageResult::Stale;
                };
                if let Some(output) = state
                    .outputs
                    .iter_mut()
                    .find(|output| output.key() == self.key)
                {
                    output.system_content_width = Some(width.0);
                }
                MessageResult::Action(())
            }
            _ => MessageResult::Stale,
        }
    }
}

#[derive(Debug)]
pub(super) struct PreferredWidth(f64);
pub(super) struct MeasureContent {
    inner: WidgetPod<dyn Widget>,
    hidden_width: f64,
    measure_width: bool,
    width: Option<f64>,
}
impl Widget for MeasureContent {
    type Action = PreferredWidth;
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
        // Hover feedback shares the clock's allotted space. Measuring it would
        // resize the surface and remove icons while their configure is pending.
        let width = self.measure_width.then(|| {
            let natural = ctx.compute_size(&mut self.inner, SizeDef::MAX, size.into());
            (natural.width + self.hidden_width).ceil()
        });
        ctx.run_layout(&mut self.inner, size);
        ctx.place_child(&mut self.inner, Point::ORIGIN);
        ctx.derive_baselines(&self.inner);
        if let Some(width) = width
            && self.width != Some(width)
        {
            self.width = Some(width);
            ctx.submit_action::<PreferredWidth>(PreferredWidth(width));
        }
    }
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.inner.id()])
    }
    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("CornerContent", id = id.trace())
    }
}
