//! The app chooser floats above the editor, retaining its recording geometry.
use crate::{state::SettingsApp, typography::label};
use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        layout::{AsUnit, Dim},
        peniko::ImageBrush,
    },
    style::Style,
    view::{CrossAxisAlignment, flex_col},
};

mod field;
mod layer;
mod option;
use field::PickerField;

#[derive(Clone, PartialEq)]
struct Choice {
    id: String,
    name: String,
    image: Option<ImageBrush>,
}

#[derive(Debug)]
enum PickerAction {
    Open,
    Query(String),
    Dismiss,
    Choose(String),
}

fn choice(state: &SettingsApp, app: &hearthspace_shell::app_catalog::DesktopApp) -> Choice {
    Choice {
        id: app.id.clone(),
        name: app.name.clone(),
        image: app
            .icon
            .as_ref()
            .and_then(|name| state.favorites.icons.get(name))
            .cloned()
            .flatten(),
    }
}

pub(super) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let favorites = &state.favorites;
    let record = favorites.recording.as_ref().unwrap();
    let selected = favorites.catalog.app_by_id(&record.app_id).unwrap();
    let picker = Picker {
        selected: choice(state, selected),
        choices: favorites
            .catalog
            .search(&favorites.query, 48)
            .iter()
            .map(|app| choice(state, app))
            .collect(),
        open: favorites.picker_open,
        query: favorites.query.clone(),
    };
    flex_col((
        label("Open this app")
            .text_size(11.0)
            .color(components::settings_material::MUTED),
        picker.height(46.0.px()).width(Dim::Stretch),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(6.0.px())
    .boxed()
}

struct Picker {
    selected: Choice,
    choices: Vec<Choice>,
    open: bool,
    query: String,
}
impl ViewMarker for Picker {}
impl View<SettingsApp, (), ViewCtx> for Picker {
    type Element = Pod<PickerField>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut SettingsApp) -> (Self::Element, ()) {
        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(PickerField::new(
                    self.selected.clone(),
                    self.choices.clone(),
                ))
            }),
            (),
        )
    }
    fn rebuild(
        &self,
        previous: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut SettingsApp,
    ) {
        PickerField::refresh(
            &mut element,
            self.selected.clone(),
            self.choices.clone(),
            self.open,
            self.query != previous.query,
        );
        element.ctx.request_compose();
    }
    fn teardown(&self, _: &mut (), ctx: &mut ViewCtx, mut element: Mut<'_, Self::Element>) {
        PickerField::dismiss(&mut element);
        ctx.teardown_action_source(element);
    }
    fn message(
        &self,
        _: &mut (),
        message: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) -> MessageResult<()> {
        if message.take_first().is_some() {
            return MessageResult::Stale;
        }
        let Some(action) = message.take_message::<PickerAction>() else {
            return MessageResult::Stale;
        };
        match *action {
            PickerAction::Open => {
                state.favorites.query.clear();
                state.favorites.picker_open = true;
            }
            PickerAction::Query(query) => {
                state.favorites.query = query;
                state.favorites.picker_open = true;
            }
            PickerAction::Dismiss => {
                state.favorites.query.clear();
                state.favorites.picker_open = false;
            }
            PickerAction::Choose(id) => state.favorites.choose_app(&id),
        }
        MessageResult::Action(())
    }
}
