//! Replace a recycled strip slot when its application changes. In-flight input
//! must not activate an app that slid into the place of a closed application.
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::widgets::SizedBox,
};

use super::{Key, ShellState};

pub(super) fn item<V: WidgetView<ShellState>>(key: Key, view: V) -> Item<V> {
    Item { key, view }
}

pub(super) struct Item<V> {
    key: Key,
    view: V,
}

impl<V> ViewMarker for Item<V> {}

impl<V: WidgetView<ShellState>> View<ShellState, (), ViewCtx> for Item<V> {
    type Element = Pod<SizedBox>;
    type ViewState = (u64, V::ViewState);

    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let (child, view_state) = ctx.with_id(ViewId::new(0), |ctx| self.view.build(ctx, state));
        (
            ctx.create_pod(SizedBox::new(child.new_widget)),
            (0, view_state),
        )
    }

    fn rebuild(
        &self,
        previous: &Self,
        (generation, view_state): &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) {
        if self.key == previous.key {
            ctx.with_id(ViewId::new(*generation), |ctx| {
                self.view.rebuild(
                    &previous.view,
                    view_state,
                    ctx,
                    SizedBox::child_mut(&mut element).unwrap().downcast(),
                    state,
                );
            });
        } else {
            ctx.with_id(ViewId::new(*generation), |ctx| {
                previous.view.teardown(
                    view_state,
                    ctx,
                    SizedBox::child_mut(&mut element).unwrap().downcast(),
                );
            });
            *generation = generation.wrapping_add(1);
            let (child, next_state) =
                ctx.with_id(ViewId::new(*generation), |ctx| self.view.build(ctx, state));
            SizedBox::set_child(&mut element, child.new_widget);
            *view_state = next_state;
        }
    }

    fn teardown(
        &self,
        (generation, view_state): &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(ViewId::new(*generation), |ctx| {
            self.view.teardown(
                view_state,
                ctx,
                SizedBox::child_mut(&mut element).unwrap().downcast(),
            );
        });
    }

    fn message(
        &self,
        (generation, view_state): &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        if message.take_first() != Some(ViewId::new(*generation)) {
            return MessageResult::Nop;
        }
        self.view.message(
            view_state,
            message,
            SizedBox::child_mut(&mut element).unwrap().downcast(),
            state,
        )
    }
}
