//! Native classes for the shared convenience views; no extra widget layer.
use crate::Palette;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        core::{DefaultProperties, PropertyStack, Selector},
        properties::{Background, BorderColor, ContentColor},
        widgets::{Label, SizedBox},
    },
};

pub(crate) const MUTED: &str = "hearthspace-muted-text";
pub(crate) const BACKGROUND: &str = "hearthspace-app-background";
pub(crate) const SIDEBAR: &str = "hearthspace-sidebar-panel";
pub(crate) const SECTION: &str = "hearthspace-settings-section";

pub(crate) fn properties(properties: &mut DefaultProperties, palette: &Palette) {
    let mut labels = PropertyStack::new();
    labels.push_layer(
        Selector::classes(&[MUTED]),
        ContentColor::new(palette.text_muted),
    );
    properties.insert_stack::<Label>(labels);
    let mut panels = PropertyStack::new();
    panels.push_layer(
        Selector::classes(&[BACKGROUND]),
        Background::Color(palette.app_background),
    );
    panels.push_layer(
        Selector::classes(&[SIDEBAR]),
        (
            Background::Color(palette.surface),
            BorderColor {
                color: palette.border,
            },
        ),
    );
    panels.push_layer(
        Selector::classes(&[SECTION]),
        (
            Background::Color(palette.paper),
            BorderColor {
                color: palette.border,
            },
        ),
    );
    properties.insert_stack::<SizedBox>(panels);
}

pub(crate) fn class<V>(inner: V, name: &'static str) -> ClassView<V> {
    ClassView { inner, name }
}
pub(crate) struct ClassView<V> {
    inner: V,
    name: &'static str,
}
impl<V> ViewMarker for ClassView<V> {}
impl<State: 'static, Action: 'static, V: WidgetView<State, Action>> View<State, Action, ViewCtx>
    for ClassView<V>
{
    type Element = Pod<V::Widget>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (mut element, view_state) = self.inner.build(ctx, state);
        element.new_widget = element.new_widget.with_class(self.name);
        (element, view_state)
    }
    fn rebuild(
        &self,
        previous: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        self.inner.rebuild(
            &previous.inner,
            view_state,
            ctx,
            element.reborrow_mut(),
            state,
        );
        if self.name != previous.name {
            element.ctx.remove_class(previous.name);
            element.ctx.add_class(self.name);
        }
    }
    fn teardown(
        &self,
        state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        self.inner.teardown(state, ctx, element);
    }
    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<Action> {
        self.inner.message(view_state, message, element, state)
    }
}
