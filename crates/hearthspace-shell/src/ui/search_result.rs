//! A named result button with explicit selection and automatic scroll reveal.
use super::{
    launcher::{self, Entry},
    search_editor::SelectionStep,
    search_focus::FocusTarget,
    search_selection,
    state::{ShellOutputInstance, ShellState},
};
use crate::ui::appearance::{ThemeStyle as _, Tone};
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{core::PointerButton, layout::AsUnit, properties::Padding, widgets::ButtonPress},
    style::Style,
};

#[path = "search_result_widget.rs"]
mod widget;
use widget::{FocusedResult, ResultButton, Reveal};

#[path = "search_result_content.rs"]
mod content;

pub(super) fn row(
    state: &ShellState,
    output: &ShellOutputInstance,
    entry: Entry,
    selected: bool,
    target: FocusTarget,
) -> impl WidgetView<ShellState, Widget: Sized> + use<> {
    let grid = launcher::is_grid(output);
    let inner = content::view(state, &entry, selected, grid);
    let key = output.key().to_owned();
    let output_name = output.output_name.clone();
    let activation = entry.clone();
    let label = if entry.is_favorite(state) {
        format!("{}, favorite", entry.label())
    } else {
        entry.label()
    };
    ResultView {
        key: output.key().into(),
        label,
        entry: Some(entry),
        selected,
        target,
        inner,
        columns: launcher::grid_columns(output),
        pressed: None,
        action: move |state: &mut ShellState| {
            launcher::select(state, &key, output_name.as_deref(), &activation)
        },
    }
    .height(if grid { 96.0 } else { 64.0 }.px())
    .padding(Padding::from_vh(
        7.0.px(),
        if grid { 2.0 } else { 8.0 }.px(),
    ))
    .theme_background(if selected {
        Tone::Subtle
    } else {
        Tone::Transparent
    })
    .corner_radius(10.0.px())
}

pub(super) fn control<V: WidgetView<ShellState>, F: Fn(&mut ShellState) + Send + Sync + 'static>(
    name: &str,
    pressed: Option<bool>,
    inner: V,
    action: F,
) -> impl WidgetView<ShellState, Widget: Sized> + use<V, F> {
    focusing_control(name, pressed, FocusTarget::default(), inner, action)
}

pub(super) fn focusing_control<
    V: WidgetView<ShellState>,
    F: Fn(&mut ShellState) + Send + Sync + 'static,
>(
    name: &str,
    pressed: Option<bool>,
    target: FocusTarget,
    inner: V,
    action: F,
) -> impl WidgetView<ShellState, Widget: Sized> + use<V, F> {
    ResultView {
        key: String::new(),
        label: name.into(),
        entry: None,
        selected: false,
        target,
        inner,
        columns: 1,
        pressed,
        action,
    }
}

struct ResultView<V, F> {
    key: String,
    label: String,
    entry: Option<Entry>,
    selected: bool,
    target: FocusTarget,
    inner: V,
    columns: usize,
    pressed: Option<bool>,
    action: F,
}
const CONTENT: ViewId = ViewId::new(0);
impl<V, F> ViewMarker for ResultView<V, F> {}
impl<V: WidgetView<ShellState>, F: Fn(&mut ShellState) + Send + Sync + 'static>
    View<ShellState, (), ViewCtx> for ResultView<V, F>
{
    type Element = Pod<ResultButton>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut ShellState) -> (Self::Element, Self::ViewState) {
        let label = self.label.clone();
        let (inner, view_state) = ctx.with_id(CONTENT, |ctx| self.inner.build(ctx, state));
        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(ResultButton {
                    label: label.clone(),
                    text: inner.new_widget.erased().to_pod(),
                    selected: self.selected,
                    target: self.target.clone(),
                    columns: self.columns,
                    pressed: self.pressed,
                    navigation: self.entry.is_some(),
                })
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
        let label = self.label.clone();
        if element.widget.label != label {
            element.widget.label = label.clone();
            element.ctx.request_accessibility_update();
        }
        element.widget.target = self.target.clone();
        element.widget.columns = self.columns;
        if previous.pressed != self.pressed {
            element.widget.pressed = self.pressed;
            element.ctx.request_accessibility_update();
        }
        ctx.with_id(CONTENT, |ctx| {
            self.inner.rebuild(
                &previous.inner,
                view_state,
                ctx,
                ResultButton::child_mut(&mut element).downcast(),
                state,
            )
        });
        if previous.selected != self.selected {
            element.widget.selected = self.selected;
            element.ctx.request_accessibility_update();
            if self.selected {
                let xilem::masonry::core::WidgetMut { widget, ctx, .. } = &mut element;
                ctx.mutate_child_later(&mut widget.text, |mut child| {
                    child.ctx.submit_untyped_action(Box::new(Reveal))
                });
            }
        }
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
                ResultButton::child_mut(&mut element).downcast(),
            )
        });
        ctx.teardown_action_source(element);
    }
    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        if message.take_first() == Some(CONTENT) {
            return self.inner.message(
                view_state,
                message,
                ResultButton::child_mut(&mut element).downcast(),
                state,
            );
        }
        if message.take_message::<FocusedResult>().is_some() {
            if let Some(entry) = &self.entry {
                search_selection::choose(state, &self.key, &entry.id());
            }
        } else if let Some(step) = message.take_message::<SelectionStep>() {
            search_selection::step(state, &self.key, step.0);
        } else if let Some(press) = message.take_message::<ButtonPress>() {
            if matches!(press.button, None | Some(PointerButton::Primary)) {
                (self.action)(state);
            }
        } else {
            return MessageResult::Stale;
        }
        MessageResult::Action(())
    }
}

pub(super) fn theme_properties(
    properties: &mut xilem::masonry::core::DefaultProperties,
    palette: hearthspace_ui_components::Palette,
    stack: xilem::masonry::core::PropertyStack,
) {
    properties.insert::<widget::ResultButton, _>(palette);
    properties.insert_stack::<widget::ResultButton>(stack);
}
