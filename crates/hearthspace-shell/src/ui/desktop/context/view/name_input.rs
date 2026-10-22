use crate::ui::{search_editor::SearchEditor, search_focus::FocusTarget, state::ShellState};
use xilem::{
    Pod, ViewCtx,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::widgets::TextAction,
};

pub(super) struct Input {
    pub text: String,
    pub target: FocusTarget,
    pub folder: bool,
}
impl ViewMarker for Input {}
impl View<ShellState, (), ViewCtx> for Input {
    type Element = Pod<SearchEditor>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut ShellState) -> (Self::Element, ()) {
        let pod = ctx.create_pod(SearchEditor::text_input(&self.text, "Name"));
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
        if self.text != previous.text && element.widget.text.text() != self.text.as_str() {
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
        let Some(action) = message.take_message::<TextAction>() else {
            return MessageResult::Stale;
        };
        match *action {
            TextAction::Changed(text) => state.desktop.menu.text = text,
            TextAction::Entered(_) => submit(state, self.folder),
            TextAction::Cancelled => {
                if let Some(context) = &state.desktop.menu.current {
                    super::super::dismiss(state, context.generation);
                }
            }
        }
        MessageResult::Action(())
    }
}

pub(super) fn submit(state: &mut ShellState, folder: bool) {
    let text = state.desktop.menu.text.clone();
    super::super::send(
        state,
        if folder {
            super::super::Action::NewFolder(text)
        } else {
            super::super::Action::Rename(text)
        },
    );
}
