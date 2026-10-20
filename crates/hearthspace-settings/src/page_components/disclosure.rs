//! A disclosure button with native focus and explicit expanded accessibility.
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{core::PointerButton, widgets::ButtonPress},
};

use crate::state::SettingsApp;

mod widget;
pub(crate) use widget::DisclosureButton;
#[cfg(test)]
mod tests;

pub(crate) fn disclosure<V, F>(
    name: impl Into<String>,
    expanded: bool,
    disabled: bool,
    child: V,
    callback: F,
) -> Disclosure<V, F>
where
    V: WidgetView<SettingsApp>,
    F: Fn(&mut SettingsApp) + Send + Sync + 'static,
{
    Disclosure {
        name: name.into(),
        expanded,
        disabled,
        child,
        callback,
    }
}

pub(crate) struct Disclosure<V, F> {
    name: String,
    expanded: bool,
    disabled: bool,
    child: V,
    callback: F,
}

const CONTENT: ViewId = ViewId::new(0);
impl<V, F> ViewMarker for Disclosure<V, F> {}
impl<V, F> View<SettingsApp, (), ViewCtx> for Disclosure<V, F>
where
    V: WidgetView<SettingsApp>,
    F: Fn(&mut SettingsApp) + Send + Sync + 'static,
{
    type Element = Pod<DisclosureButton>;
    type ViewState = V::ViewState;

    fn build(
        &self,
        ctx: &mut ViewCtx,
        state: &mut SettingsApp,
    ) -> (Self::Element, Self::ViewState) {
        let (child, view_state) = ctx.with_id(CONTENT, |ctx| self.child.build(ctx, state));
        let element = ctx.with_action_widget(|ctx| {
            let mut element = ctx.create_pod(DisclosureButton::new(
                self.name.clone(),
                self.expanded,
                child.new_widget,
            ));
            element.new_widget.options.disabled = self.disabled;
            element
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
        if self.expanded != previous.expanded || self.name != previous.name {
            element.widget.expanded = self.expanded;
            element.widget.name = self.name.clone();
            element.ctx.request_accessibility_update();
        }
        if self.disabled != previous.disabled {
            element.ctx.set_disabled(self.disabled);
        }
        ctx.with_id(CONTENT, |ctx| {
            self.child.rebuild(
                &previous.child,
                view_state,
                ctx,
                DisclosureButton::child_mut(&mut element).downcast(),
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
            self.child.teardown(
                view_state,
                ctx,
                DisclosureButton::child_mut(&mut element).downcast(),
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
            Some(CONTENT) => self.child.message(
                view_state,
                message,
                DisclosureButton::child_mut(&mut element).downcast(),
                state,
            ),
            None => {
                if let Some(press) = message.take_message::<ButtonPress>() {
                    if !self.disabled && matches!(press.button, None | Some(PointerButton::Primary))
                    {
                        (self.callback)(state);
                    }
                    MessageResult::Action(())
                } else {
                    MessageResult::Stale
                }
            }
            _ => MessageResult::Stale,
        }
    }
}
