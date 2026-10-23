//! Turn a native label without losing its measured size or accessibility text.
use crate::ui::state::ShellState;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef,
            RegisterCtx, Widget, WidgetPod,
        },
        imaging::Painter,
        kurbo::{Affine, Axis, Point, Size},
        layout::{LayoutSize, LenReq, Length},
    },
};

pub(super) fn vertical_text<V: WidgetView<ShellState>>(label: V) -> impl WidgetView<ShellState> {
    VerticalText {
        // The same clockwise direction as the reference's vertical-rl Latin text.
        inner: label.transform(Affine::new([0.0, 1.0, -1.0, 0.0, 0.0, 0.0])),
    }
}

struct VerticalText<V> {
    inner: V,
}

const CONTENT: ViewId = ViewId::new(0);
impl<V> ViewMarker for VerticalText<V> {}
impl<V: WidgetView<ShellState>> View<ShellState, (), ViewCtx> for VerticalText<V> {
    type Element = Pod<TextLayout>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let (child, view_state) = ctx.with_id(CONTENT, |ctx| self.inner.build(ctx, state));
        (
            ctx.create_pod(TextLayout {
                child: child.new_widget.erased().to_pod(),
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
        ctx.with_id(CONTENT, |ctx| {
            self.inner.rebuild(
                &previous.inner,
                view_state,
                ctx,
                element.ctx.get_mut(&mut element.widget.child).downcast(),
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
            self.inner.teardown(
                view_state,
                ctx,
                element.ctx.get_mut(&mut element.widget.child).downcast(),
            );
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
                element.ctx.get_mut(&mut element.widget.child).downcast(),
                state,
            ),
            _ => MessageResult::Stale,
        }
    }
}

struct TextLayout {
    child: WidgetPod<dyn Widget>,
}

impl Widget for TextLayout {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
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
            LayoutSize::maybe(axis, cross),
            axis.cross(),
            cross,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, Size::new(size.height, size.width));
        ctx.place_child(&mut self.child, Point::new(size.width, 0.0));
    }

    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}
