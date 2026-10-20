//! Touch scrolling around Masonry's existing Portal, retaining its normal inputs.

use std::marker::PhantomData;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{core::Widget, widgets::Portal},
};

mod widget;
pub use widget::TouchScroll;
#[cfg(test)]
mod tests;

/// A Portal with one-finger scrolling over its background and button rows.
/// Captured controls such as text editors, sliders and drawing pads retain their
/// direct manipulation. Mouse, wheel, keyboard and accessibility behavior comes
/// from the underlying Masonry Portal.
pub fn touch_portal<Child, State, Action>(child: Child) -> TouchPortal<Child, State, Action>
where
    State: 'static,
    Child: WidgetView<State, Action>,
{
    TouchPortal {
        child,
        horizontal: false,
        vertical: false,
        fill: false,
        marker: PhantomData,
    }
}

#[must_use = "Views must be provided to Xilem to have an effect."]
pub struct TouchPortal<Child, State, Action> {
    child: Child,
    horizontal: bool,
    vertical: bool,
    fill: bool,
    marker: PhantomData<fn(State) -> Action>,
}

impl<Child, State, Action> TouchPortal<Child, State, Action> {
    pub fn constrain_horizontal(mut self, constrain: bool) -> Self {
        self.horizontal = constrain;
        self
    }
    pub fn constrain_vertical(mut self, constrain: bool) -> Self {
        self.vertical = constrain;
        self
    }
    pub fn must_fill(mut self, fill: bool) -> Self {
        self.fill = fill;
        self
    }
}

impl<Child, State, Action> ViewMarker for TouchPortal<Child, State, Action> {}

impl<Child, State, Action> View<State, Action, ViewCtx> for TouchPortal<Child, State, Action>
where
    Child: WidgetView<State, Action>,
    State: 'static,
    Action: 'static,
{
    type Element = Pod<TouchScroll<Child::Widget>>;
    type ViewState = Child::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.child.build(ctx, state);
        let portal = Portal::new(child.new_widget)
            .constrain_horizontal(self.horizontal)
            .constrain_vertical(self.vertical)
            .content_must_fill(self.fill);
        (
            ctx.create_pod(TouchScroll::new(
                portal.prepare(),
                self.horizontal,
                self.vertical,
            )),
            child_state,
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        element.widget.horizontal = self.horizontal;
        element.widget.vertical = self.vertical;
        let mut portal = TouchScroll::portal_mut(&mut element);
        if self.horizontal != prev.horizontal {
            Portal::set_constrain_horizontal(&mut portal, self.horizontal);
        }
        if self.vertical != prev.vertical {
            Portal::set_constrain_vertical(&mut portal, self.vertical);
        }
        if self.fill != prev.fill {
            Portal::set_content_must_fill(&mut portal, self.fill);
        }
        self.child.rebuild(
            &prev.child,
            view_state,
            ctx,
            Portal::child_mut(&mut portal),
            state,
        );
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut portal = TouchScroll::portal_mut(&mut element);
        self.child
            .teardown(view_state, ctx, Portal::child_mut(&mut portal));
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<Action> {
        let mut portal = TouchScroll::portal_mut(&mut element);
        self.child
            .message(view_state, message, Portal::child_mut(&mut portal), state)
    }
}
