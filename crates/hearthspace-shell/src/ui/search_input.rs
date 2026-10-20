use super::{
    search_editor::{SearchEditor, SelectionStep},
    search_focus::FocusTarget,
    search_selection,
    state::{ShellOutputInstance, ShellState},
};
use crate::ui::appearance::{ThemeStyle as _, Tone};
use hearthspace_ui_components::icons::{Icon, icon};
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{layout::AsUnit, properties::Padding, widgets::TextAction},
    style::Style,
    view::{FlexExt, flex_row},
};

pub(super) fn input(
    output: &ShellOutputInstance,
    target: FocusTarget,
) -> impl WidgetView<ShellState> + use<> {
    let editor = SearchInput {
        key: output.key().to_string(),
        text: output.query.clone(),
        target: target.clone(),
    }
    .height(44.0.px())
    .padding(Padding::from_vh(10.0.px(), 0.0.px()));
    let key = output.key().to_owned();
    let clear = (!output.query.is_empty()).then(|| {
        super::search_result::focusing_control(
            "Clear search",
            None,
            target,
            icon(Icon::X, 17.0).theme_color(Tone::Muted),
            move |state| state.change_query(&key, String::new()),
        )
        .height(44.0.px())
        .padding(12.0.px())
        .corner_radius(10.0.px())
    });
    flex_row((
        icon(Icon::Search, 22.0).theme_color(Tone::Accent),
        editor.flex(1.0),
        clear,
    ))
    .gap(12.0.px())
    .height(54.0.px())
}

struct SearchInput {
    key: String,
    text: String,
    target: FocusTarget,
}
impl ViewMarker for SearchInput {}
impl View<ShellState, (), ViewCtx> for SearchInput {
    type Element = Pod<SearchEditor>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut ShellState) -> (Self::Element, ()) {
        let pod = ctx.create_pod(SearchEditor::new(&self.text));
        ctx.record_action_source(pod.new_widget.id());
        *self.target.lock().unwrap() = Some(pod.new_widget.id());
        (pod, ())
    }
    fn rebuild(
        &self,
        previous: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut ShellState,
    ) {
        *self.target.lock().unwrap() = Some(element.id());
        // A layout action may rebuild before the editor's Changed action has
        // reached state. Only an actual model change may reset the editor.
        if previous.text != self.text && element.widget.text.text() != self.text.as_str() {
            element.widget.reset_text(&self.text);
            element.ctx.request_layout();
            element.ctx.request_render();
        }
    }
    fn teardown(&self, _: &mut (), ctx: &mut ViewCtx, element: Mut<'_, Self::Element>) {
        *self.target.lock().unwrap() = None;
        ctx.teardown_action_source(element);
    }
    fn message(
        &self,
        _: &mut (),
        message: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        if let Some(step) = message.take_message::<SelectionStep>() {
            let columns = state
                .outputs
                .iter()
                .find(|output| output.key() == self.key)
                .map_or(1, |output| super::launcher::grid_columns(output) as i32);
            search_selection::step(state, &self.key, step.0 * columns);
        } else if let Some(action) = message.take_message::<TextAction>() {
            match *action {
                TextAction::Changed(text) => state.change_query(&self.key, text),
                TextAction::Entered(_) => search_selection::activate(state, &self.key),
                TextAction::Cancelled => state.dismiss_search(&self.key),
            }
        } else {
            return MessageResult::Stale;
        }
        MessageResult::Action(())
    }
}
