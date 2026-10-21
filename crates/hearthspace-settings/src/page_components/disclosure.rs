//! A disclosure button with native focus and explicit expanded accessibility.
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        core::{PointerButton, WidgetId},
        widgets::ButtonPress,
    },
};

use crate::state::SettingsApp;

mod widget;
pub(crate) use widget::DisclosureButton;

/// Native identity for retained navigation controls, consumed by their layout.
#[derive(Debug)]
pub(crate) struct NavigationControl {
    pub(crate) controls: Option<WidgetId>,
    pub(crate) current_page: Option<bool>,
}

#[derive(Debug)]
pub(crate) struct NavigationInput {
    pub(crate) keyboard: bool,
}

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
        selected: None,
        current_page: None,
        controls: None,
        disabled,
        child,
        callback,
    }
}

pub(crate) struct Disclosure<V, F> {
    name: String,
    expanded: bool,
    selected: Option<bool>,
    current_page: Option<bool>,
    controls: Option<WidgetId>,
    disabled: bool,
    child: V,
    callback: F,
}

impl<V, F> Disclosure<V, F> {
    pub(crate) fn current_page(mut self, current: bool) -> Self {
        self.current_page = Some(current);
        self
    }

    pub(crate) fn controls(mut self, target: WidgetId) -> Self {
        self.controls = Some(target);
        self
    }

    pub(crate) fn selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }
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
            let mut widget =
                DisclosureButton::new(self.name.clone(), self.expanded, child.new_widget);
            widget.selected = self.selected;
            widget.current_page = self.current_page;
            widget.controls = self.controls;
            let mut element = ctx.create_pod(widget);
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
        if self.expanded != previous.expanded
            || self.name != previous.name
            || self.selected != previous.selected
            || self.current_page != previous.current_page
            || self.controls != previous.controls
        {
            element.widget.expanded = self.expanded;
            element.widget.name = self.name.clone();
            element.widget.selected = self.selected;
            element.widget.current_page = self.current_page;
            element.widget.controls = self.controls;
            element.ctx.request_accessibility_update();
            if (self.current_page != previous.current_page || self.controls != previous.controls)
                && let Some(identity) = element.widget.navigation_identity()
            {
                element.ctx.submit_untyped_action(Box::new(identity));
            }
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
