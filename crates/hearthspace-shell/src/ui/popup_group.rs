//! A semantic group keeps scrollable controls available to assistive tools.
//!
//! AccessKit filters direct descendants of a clipping scroll view by their
//! bounds. When only that viewport grows, unchanged offscreen rows do not get
//! registered before a later removal. A real content group exposes all of its
//! children, including scrollable offscreen controls, with a stable hierarchy.
use super::state::ShellState;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NoAction, PaintCtx,
            PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Widget, WidgetId, WidgetMut,
            WidgetPod,
        },
        imaging::Painter,
        kurbo::{Axis, Point, Size},
        layout::{LenReq, Length},
    },
};

pub(super) fn group<V: WidgetView<ShellState>>(label: &'static str, inner: V) -> GroupView<V> {
    GroupView {
        label,
        inner,
        reset_focus_on_deactivation: false,
        accessibility_bounds: true,
    }
}
pub(super) struct GroupView<V> {
    label: &'static str,
    inner: V,
    reset_focus_on_deactivation: bool,
    accessibility_bounds: bool,
}
impl<V> GroupView<V> {
    /// Corners are separate native windows; do not leave a focused control in
    /// each previously visited corner. Editors and ordinary popups retain focus.
    pub(super) fn reset_focus_on_deactivation(mut self) -> Self {
        self.reset_focus_on_deactivation = true;
        self
    }

    /// A logical collection remains accessible even while its scroll viewport
    /// has no space during popup configuration. Its children keep their bounds.
    pub(super) fn without_accessibility_bounds(mut self) -> Self {
        self.accessibility_bounds = false;
        self
    }
}
impl<V> ViewMarker for GroupView<V> {}
const CONTENT: ViewId = ViewId::new(0);
impl<V: WidgetView<ShellState>> View<ShellState, (), ViewCtx> for GroupView<V> {
    type Element = Pod<PopupGroup>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let (inner, view_state) = ctx.with_id(CONTENT, |ctx| self.inner.build(ctx, state));
        (
            ctx.create_pod(PopupGroup {
                label: self.label,
                inner: inner.new_widget.erased().to_pod(),
                reset_focus_on_deactivation: self.reset_focus_on_deactivation,
                accessibility_bounds: self.accessibility_bounds,
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
        if self.label != previous.label {
            element.widget.label = self.label;
            element.ctx.request_accessibility_update();
        }
        element.widget.reset_focus_on_deactivation = self.reset_focus_on_deactivation;
        if self.accessibility_bounds != previous.accessibility_bounds {
            element.widget.accessibility_bounds = self.accessibility_bounds;
            element.ctx.request_accessibility_update();
        }
        ctx.with_id(CONTENT, |ctx| {
            self.inner.rebuild(
                &previous.inner,
                view_state,
                ctx,
                PopupGroup::child_mut(&mut element).downcast(),
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
                PopupGroup::child_mut(&mut element).downcast(),
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
                PopupGroup::child_mut(&mut element).downcast(),
                state,
            ),
            _ => MessageResult::Stale,
        }
    }
}

pub(super) struct PopupGroup {
    label: &'static str,
    inner: WidgetPod<dyn Widget>,
    reset_focus_on_deactivation: bool,
    accessibility_bounds: bool,
}
impl PopupGroup {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.inner)
    }
}
impl Widget for PopupGroup {
    type Action = NoAction;
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.inner);
    }
    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if self.reset_focus_on_deactivation
            && matches!(event, TextEvent::WindowFocusChange(false))
            && ctx.has_focus_target()
        {
            ctx.resign_focus();
        }
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        cross: Option<Length>,
    ) -> Length {
        let length = ctx.redirect_measurement(&mut self.inner, axis, cross);
        // Portal compares intrinsic height before pixel snapping, while the
        // resize observer reports the snapped layout height. Round upward at
        // their shared boundary so a fractional remainder (e.g. 0.0625px)
        // cannot create a scrollbar in a naturally sized popup.
        if axis == Axis::Vertical {
            Length::px(length.get().ceil())
        } else {
            length
        }
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.inner, size);
        ctx.place_child(&mut self.inner, Point::ORIGIN);
        ctx.derive_baselines(&self.inner);
    }
    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
    fn accessibility_role(&self) -> Role {
        Role::Group
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.label);
        if !self.accessibility_bounds {
            node.clear_bounds();
        }
    }
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.inner.id()])
    }
    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("PopupGroup", id = id.trace())
    }
}
