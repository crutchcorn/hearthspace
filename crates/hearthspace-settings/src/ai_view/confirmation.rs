//! The destructive AI action is a modal, separate from the page's layout.
use crate::state::SettingsApp;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
};

mod button;
mod layer;
mod owner;
mod panel;
use owner::ConfirmationOwner;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Decision {
    Keep,
    Delete,
}

pub(super) fn modal<V: WidgetView<SettingsApp>>(inner: V, open: bool) -> Confirmation<V> {
    Confirmation { inner, open }
}

pub(super) struct Confirmation<V> {
    inner: V,
    open: bool,
}

const CONTENT: ViewId = ViewId::new(0);
impl<V> ViewMarker for Confirmation<V> {}
impl<V: WidgetView<SettingsApp>> View<SettingsApp, (), ViewCtx> for Confirmation<V> {
    type Element = Pod<ConfirmationOwner>;
    type ViewState = V::ViewState;

    fn build(
        &self,
        ctx: &mut ViewCtx,
        state: &mut SettingsApp,
    ) -> (Self::Element, Self::ViewState) {
        let (child, view_state) = ctx.with_id(CONTENT, |ctx| self.inner.build(ctx, state));
        let element = ctx.with_action_widget(|ctx| {
            ctx.create_pod(ConfirmationOwner::new(child.new_widget, self.open))
        });
        (element, view_state)
    }

    fn rebuild(
        &self,
        previous: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) {
        ctx.with_id(CONTENT, |ctx| {
            self.inner.rebuild(
                &previous.inner,
                view_state,
                ctx,
                ConfirmationOwner::child_mut(&mut element).downcast(),
                state,
            );
        });
        element.widget.open = self.open;
        ConfirmationOwner::sync_layer(&mut element);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        element.widget.open = false;
        ConfirmationOwner::sync_layer(&mut element);
        ctx.with_id(CONTENT, |ctx| {
            self.inner.teardown(
                view_state,
                ctx,
                ConfirmationOwner::child_mut(&mut element).downcast(),
            );
        });
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) -> MessageResult<()> {
        match message.take_first() {
            Some(CONTENT) => self.inner.message(
                view_state,
                message,
                ConfirmationOwner::child_mut(&mut element).downcast(),
                state,
            ),
            None => match message.take_message::<Decision>() {
                Some(decision) => {
                    if state.ai.confirm_off {
                        match *decision {
                            Decision::Keep => state.ai.confirm_off = false,
                            Decision::Delete => state.ai.confirm_removal(),
                        }
                    }
                    MessageResult::Action(())
                }
                None => MessageResult::Stale,
            },
            _ => MessageResult::Stale,
        }
    }
}

pub(crate) fn theme_properties(
    properties: &mut xilem::masonry::core::DefaultProperties,
    palette: hearthspace_ui_components::Palette,
) {
    properties.insert::<panel::ConfirmationPanel, _>(palette);
    properties.insert::<button::ConfirmationButton, _>(palette);
    properties.insert::<layer::ConfirmationLayer, _>(palette);
}
