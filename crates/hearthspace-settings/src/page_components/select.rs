//! Small Settings choices share a native combobox and separate popup layer.
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{layout::AsUnit, properties::Padding},
    style::Style,
    view::{MainAxisAlignment, flex_row},
};

use crate::{state::SettingsApp, typography::label};

mod button;
mod layer;
mod option;
use button::SettingsSelectButton;

pub(crate) fn select<F>(
    palette: components::Palette,
    name: &'static str,
    options: Vec<String>,
    selected: usize,
    disabled: bool,
    callback: F,
) -> impl WidgetView<SettingsApp, Widget: Sized>
where
    F: Fn(&mut SettingsApp, usize) + Send + Sync + 'static,
{
    assert!(!options.is_empty() && selected < options.len());
    SettingsSelect {
        name,
        child: flex_row((
            label(options[selected].clone()).text_size(11.0),
            icon(Icon::ChevronDown, 12.0).color(palette.muted),
        ))
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .gap(8.0.px()),
        options,
        selected,
        disabled,
        callback,
    }
    .padding(Padding::from_vh(6.0.px(), 8.0.px()))
}

struct SettingsSelect<V, F> {
    name: &'static str,
    options: Vec<String>,
    selected: usize,
    disabled: bool,
    child: V,
    callback: F,
}

const CONTENT: ViewId = ViewId::new(0);
impl<V, F> ViewMarker for SettingsSelect<V, F> {}
impl<V: WidgetView<SettingsApp>, F> View<SettingsApp, (), ViewCtx> for SettingsSelect<V, F>
where
    F: Fn(&mut SettingsApp, usize) + Send + Sync + 'static,
{
    type Element = Pod<SettingsSelectButton>;
    type ViewState = V::ViewState;

    fn build(
        &self,
        ctx: &mut ViewCtx,
        state: &mut SettingsApp,
    ) -> (Self::Element, Self::ViewState) {
        let (child, view_state) = ctx.with_id(CONTENT, |ctx| self.child.build(ctx, state));
        let element = ctx.with_action_widget(|ctx| {
            let mut element = ctx.create_pod(SettingsSelectButton::new(
                self.name.into(),
                self.options.clone(),
                self.selected,
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
        if self.selected != previous.selected
            || self.options != previous.options
            || self.name != previous.name
            || self.disabled
        {
            SettingsSelectButton::dismiss(&mut element);
            element.widget.selected = self.selected;
            element.widget.name = self.name.into();
            element.widget.options = self.options.clone();
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
                SettingsSelectButton::child_mut(&mut element).downcast(),
                state,
            );
        });
        // Other page content may reflow while this control keeps the same size.
        // Refresh an open popup's anchor once the new transforms are composed.
        element.ctx.request_compose();
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        // The attached layer is also removed by Masonry when this widget leaves
        // the tree. Explicit dismissal updates its registration before teardown.
        SettingsSelectButton::dismiss(&mut element);
        ctx.with_id(CONTENT, |ctx| {
            self.child.teardown(
                view_state,
                ctx,
                SettingsSelectButton::child_mut(&mut element).downcast(),
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
                SettingsSelectButton::child_mut(&mut element).downcast(),
                state,
            ),
            None => {
                if let Some(index) = message.take_message::<usize>() {
                    if !self.disabled {
                        (self.callback)(state, *index);
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

pub(crate) fn theme_properties(
    properties: &mut xilem::masonry::core::DefaultProperties,
    palette: hearthspace_ui_components::Palette,
) {
    properties.insert::<button::SettingsSelectButton, _>(palette);
    properties.insert::<option::SelectOption, _>(palette);
    layer::theme_properties(properties, palette);
}
