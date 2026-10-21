use super::{ShellState, activate, change_query, close, material, submit_command};
use crate::ui::{
    search_editor::{SearchEditor, SelectionStep},
    search_focus::{FocusTarget, focus_on_activation},
};
use hearthspace_ui_components::icons::{Icon, icon};
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{layout::AsUnit, properties::Padding, widgets::TextAction},
    style::Style,
    view::{CrossAxisAlignment, FlexExt, flex_col, flex_row, label, sized_box},
};

pub(super) fn view(state: &ShellState) -> impl WidgetView<ShellState> + use<> {
    let search = &state.canvas_search;
    let palette = state.appearance.palette;
    let target = search.target.clone();
    let input = Input {
        text: search.query.clone(),
        target: target.clone(),
    }
    .height(44.0.px())
    .padding(Padding::from_vh(12.0.px(), 8.0.px()))
    .color(if search.query.is_empty() {
        palette.muted
    } else {
        palette.text
    });
    let close_button = hearthspace_ui_components::settings_button(
        crate::ui::search_result::focusing_control(
            "Close canvas search",
            None,
            target.clone(),
            icon(Icon::X, 18.0).color(palette.muted),
            close,
        )
        .height(44.0.px())
        .width(44.0.px())
        .padding(12.0.px())
        .corner_radius(15.0.px()),
        hearthspace_ui_components::SettingsButton::Bare,
        false,
    );
    let row = sized_box(
        flex_row((
            icon(Icon::Search, 20.0).color(palette.muted),
            input.flex(1.0),
            close_button,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(4.0.px()),
    )
    .padding(Padding {
        left: 16.0.px(),
        right: 4.0.px(),
        top: 4.0.px(),
        bottom: 4.0.px(),
    })
    .height(52.0.px());
    let empty = || {
        (search.snapshot.result_count == 0).then(|| {
            flex_col((
                (!search.snapshot.results_above).then(|| {
                    sized_box(label(""))
                        .height(1.0.px())
                        .background_color(palette.border)
                }),
                sized_box(
                    crate::ui::popup_group::group(
                        "",
                        label("No matches").text_size(13.0).color(palette.muted),
                    )
                    .status(),
                )
                .height(41.0.px())
                .padding(Padding::from_vh(12.0.px(), 16.0.px())),
                search.snapshot.results_above.then(|| {
                    sized_box(label(""))
                        .height(1.0.px())
                        .background_color(palette.border)
                }),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(0.0.px())
        })
    };
    let content = flex_col((
        search.snapshot.results_above.then(empty).flatten(),
        row,
        (!search.snapshot.results_above).then(empty).flatten(),
        sized_box(
            crate::ui::popup_group::group(
                search
                    .snapshot
                    .results
                    .iter()
                    .enumerate()
                    .find(|(_, result)| Some(&result.id) == search.snapshot.selected_id.as_ref())
                    .map(|(index, result)| {
                        format!(
                            "{}, {} of {}",
                            result.title,
                            index + 1,
                            search.snapshot.result_count
                        )
                    })
                    .unwrap_or_default(),
                label(""),
            )
            .status()
            .without_accessibility_bounds(),
        )
        .height(0.0.px()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(0.0.px());
    let panel = material::panel(sized_box(content).padding(1.0.px()), false, palette);
    focus_on_activation(
        crate::ui::popup_group::group("Canvas search", panel),
        target,
        String::new(),
    )
    .with_dismiss(|state, _| close(state))
}

struct Input {
    text: String,
    target: FocusTarget,
}
impl ViewMarker for Input {}
impl View<ShellState, (), ViewCtx> for Input {
    type Element = Pod<SearchEditor>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut ShellState) -> (Self::Element, ()) {
        let pod = ctx.create_pod(SearchEditor::configured(
            &self.text,
            "Search the canvas…",
            "Search apps, windows, files, folders, controls, and tools",
            16.0,
        ));
        ctx.record_action_source(pod.new_widget.id());
        *self.target.lock().unwrap() = Some(pod.new_widget.id());
        (pod, ())
    }
    fn rebuild(
        &self,
        _: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut ShellState,
    ) {
        *self.target.lock().unwrap() = Some(element.id());
        if element.widget.text.text() != self.text.as_str() {
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
            submit_command(
                state,
                format!(
                    "canvas-search-step {}",
                    serde_json::json!({"session":state.canvas_search.snapshot.session,"step":step.0})
                ),
            );
        } else if let Some(action) = message.take_message::<TextAction>() {
            match *action {
                TextAction::Changed(text) => change_query(state, text),
                TextAction::Entered(_) => activate(state, None),
                TextAction::Cancelled => close(state),
            }
        } else {
            return MessageResult::Stale;
        }
        MessageResult::Action(())
    }
}
