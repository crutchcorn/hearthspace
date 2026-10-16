use super::{
    favorite_actions,
    favorite_state::Mode,
    icons::{Icon, icon_button},
    search_editor::{SearchEditor, SelectionStep},
    search_focus::FocusTarget,
    search_selection,
    state::{ShellOutputInstance, ShellState},
};
use hearthspace_ui_components as components;
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        layout::AsUnit,
        properties::{CaretColor, Padding, SelectionColor},
        widgets::TextAction,
    },
    style::Style,
    view::{FlexExt, flex_row, sized_box},
};

pub(super) fn input(
    output: &ShellOutputInstance,
    target: FocusTarget,
) -> impl WidgetView<ShellState> + use<> {
    let draw_key = output.key().to_owned();
    let settings_key = draw_key.clone();
    let active = output.drawing.active();
    let editor = SearchInput {
        key: output.key().to_string(),
        text: output.query.clone(),
        target,
    }
    .height(34.0.px())
    .color(components::TEXT)
    .padding(Padding::from_vh(6.0.px(), 10.0.px()));
    sized_box(
        flex_row((
            editor.flex(1.0),
            icon_button(Icon::Draw, "Draw to launch", active, move |state| {
                if active {
                    favorite_actions::cancel(state, &draw_key);
                } else {
                    favorite_actions::start(state, &draw_key, Mode::Launch);
                }
            }),
            icon_button(Icon::Settings, "Settings", false, move |state| {
                favorite_actions::manage(state, &settings_key)
            }),
        ))
        .gap(2.0.px()),
    )
    .background_color(components::SURFACE)
    .border(components::BORDER, 1.0.px())
    .corner_radius(5.0.px())
    .padding(2.0.px())
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
        let mut pod = ctx.create_pod(SearchEditor::new(&self.text));
        pod.new_widget = pod.new_widget.with_props((
            CaretColor {
                color: components::TEXT,
            },
            SelectionColor {
                color: components::ACCENT_SUBTLE,
            },
        ));
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
            search_selection::step(state, &self.key, step.0);
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
