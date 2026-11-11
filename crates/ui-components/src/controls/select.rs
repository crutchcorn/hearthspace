//! Small Settings choices share a native combobox and separate popup layer.
use crate::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{
        layout::{AsUnit, Dim},
        properties::Padding,
    },
    style::Style,
    view::{MainAxisAlignment, flex_row},
};

use xilem::view::label;

mod button;
mod layer;
mod option;
use button::SettingsSelectButton;

fn select_inner<State: 'static, F>(
    palette: components::Palette,
    name: &'static str,
    options: Vec<String>,
    selected: usize,
    disabled: bool,
    callback: F,
    compact: bool,
) -> impl WidgetView<State, Widget: Sized>
where
    F: Fn(&mut State, usize) + Send + Sync + 'static,
{
    assert!(!options.is_empty() && selected < options.len());
    SettingsSelect {
        name,
        font_size: if compact { 11.0 } else { 12.0 },
        child: flex_row((
            label(options[selected].clone())
                .font("Liberation Sans")
                .text_size(if compact { 11.0 } else { 12.0 })
                .color(
                    palette
                        .text
                        .multiply_alpha(if disabled { 0.5 } else { 1.0 }),
                ),
            icon(Icon::ChevronDown, if compact { 12.0 } else { 16.0 }).color(
                palette
                    .muted
                    .multiply_alpha(if disabled { 0.5 } else { 1.0 }),
            ),
        ))
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .gap(8.0.px()),
        options,
        selected,
        disabled,
        callback,
    }
    .height(if compact { Dim::Auto } else { 42.0.px().into() })
    .padding(Padding::from_vh(
        if compact { 6.0 } else { 10.0 }.px(),
        if compact { 8.0 } else { 14.0 }.px(),
    ))
}

struct SettingsSelect<V, F> {
    name: &'static str,
    font_size: f32,
    options: Vec<String>,
    selected: usize,
    disabled: bool,
    child: V,
    callback: F,
}

const CONTENT: ViewId = ViewId::new(0);
impl<V, F> ViewMarker for SettingsSelect<V, F> {}
impl<State: 'static, V: WidgetView<State>, F> View<State, (), ViewCtx> for SettingsSelect<V, F>
where
    F: Fn(&mut State, usize) + Send + Sync + 'static,
{
    type Element = Pod<SettingsSelectButton>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, view_state) = ctx.with_id(CONTENT, |ctx| self.child.build(ctx, state));
        let element = ctx.with_action_widget(|ctx| {
            let mut element = ctx.create_pod(SettingsSelectButton::new(
                self.name.into(),
                self.options.clone(),
                self.selected,
                self.font_size,
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
        state: &mut State,
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
        state: &mut State,
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
    palette: crate::Palette,
) {
    properties.insert::<button::SettingsSelectButton, _>(palette);
    properties.insert::<option::SelectOption, _>(palette);
    layer::theme_properties(properties, palette);
}

pub fn select<State: 'static, F: Fn(&mut State, usize) + Send + Sync + 'static>(
    palette: components::Palette,
    name: &'static str,
    options: Vec<String>,
    selected: usize,
    disabled: bool,
    callback: F,
) -> impl WidgetView<State, Widget: Sized> {
    select_inner(palette, name, options, selected, disabled, callback, false)
}
pub fn compact_select<State: 'static, F: Fn(&mut State, usize) + Send + Sync + 'static>(
    palette: components::Palette,
    name: &'static str,
    options: Vec<String>,
    selected: usize,
    disabled: bool,
    callback: F,
) -> impl WidgetView<State, Widget: Sized> {
    select_inner(palette, name, options, selected, disabled, callback, true)
}
