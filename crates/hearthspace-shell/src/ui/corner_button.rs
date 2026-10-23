//! Scope native focus styling to buttons painted on the translucent corners.
use super::state::ShellState;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
};

pub(super) fn focus<V: WidgetView<ShellState>>(inner: V) -> FocusView<V> {
    FocusView { inner }
}

pub(super) struct FocusView<V> {
    inner: V,
}

impl<V> ViewMarker for FocusView<V> {}

impl<V: WidgetView<ShellState>> View<ShellState, (), ViewCtx> for FocusView<V> {
    type Element = Pod<V::Widget>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let (mut element, view_state) = self.inner.build(ctx, state);
        element.new_widget = element
            .new_widget
            .with_class(hearthspace_ui_components::TRANSLUCENT_FOCUS_CLASS);
        (element, view_state)
    }

    fn rebuild(
        &self,
        previous: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) {
        self.inner
            .rebuild(&previous.inner, view_state, ctx, element, state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        self.inner.teardown(view_state, ctx, element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        self.inner.message(view_state, message, element, state)
    }
}
