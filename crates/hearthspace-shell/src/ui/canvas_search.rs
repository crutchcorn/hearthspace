//! The compositor arranges search results; this native surface owns text input.
use super::{search_focus::FocusTarget, state::ShellState};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::{Notify, mpsc, watch};
use xilem::{WindowId, WindowView, dpi::LogicalSize};

#[path = "canvas_search/controls.rs"]
pub(super) mod controls;
#[path = "canvas_search/field.rs"]
mod field;
#[path = "canvas_search/material.rs"]
pub(super) mod material;
#[path = "canvas_search/motion_preferences.rs"]
mod motion_preferences;
#[path = "canvas_search/task.rs"]
mod worker;
pub(super) fn task()
-> impl xilem::core::View<ShellState, (), xilem::ViewCtx, Element = xilem::core::NoElement> + use<>
{
    xilem::core::fork(worker::task(), motion_preferences::task())
}
#[cfg(test)]
#[path = "canvas_search/tests.rs"]
mod tests;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub(super) struct Snapshot {
    pub version: u32,
    pub revision: u64,
    pub mode: bool,
    pub active: bool,
    pub session: u64,
    pub output: Option<String>,
    pub query: String,
    pub handwriting: bool,
    pub field: Bounds,
    pub results_above: bool,
    pub handwriting_height: f64,
    pub result_count: usize,
    pub selected_id: Option<String>,
    pub hovered_id: Option<String>,
    pub results: Vec<ResultEntry>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub(super) struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct ResultEntry {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub selected: bool,
}

pub(super) struct State {
    pub snapshot: Snapshot,
    pub result_windows: std::collections::BTreeMap<String, WindowId>,
    pub query: String,
    pub handwriting_open: bool,
    pub error: Option<String>,
    pub refresh: Arc<Notify>,
    requests: Option<mpsc::Sender<String>>,
    edits: Option<watch::Sender<Option<worker::Edit>>>,
    pending_edit: Option<worker::Edit>,
    window_id: WindowId,
    target: FocusTarget,
}

impl Default for State {
    fn default() -> Self {
        Self {
            snapshot: Snapshot::default(),
            result_windows: controls::window_ids(),
            query: String::new(),
            handwriting_open: false,
            error: None,
            refresh: Arc::default(),
            requests: None,
            edits: None,
            pending_edit: None,
            window_id: WindowId::next(),
            target: FocusTarget::default(),
        }
    }
}

pub(super) fn submit_command(state: &mut ShellState, command: String) {
    if let Some(sender) = &state.canvas_search.requests
        && let Err(error) = sender.try_send(command)
    {
        state.canvas_search.error = Some(format!("Search request could not be queued: {error}"));
    }
}

pub(super) fn button(state: &ShellState) -> impl xilem::WidgetView<ShellState> + use<> {
    super::icons::icon_button(
        super::icons::Icon::Search,
        "Search canvas",
        state.canvas_search.snapshot.mode && !state.frame_layout.snapshot.editing,
        |state| {
            state.dismiss_panels();
            submit_command(state, "canvas-search-tool".into());
        },
    )
}

pub(super) fn windows(state: &ShellState) -> Vec<WindowView<ShellState>> {
    let search = &state.canvas_search;
    // Keep the small registered cards warm. The compositor hides their stable
    // surfaces outside search; rebuilding renderers on each open stalls input.
    let mut windows = controls::windows(state);
    if !search.snapshot.active {
        return windows;
    }
    windows.push(super::popup::window(
        search.window_id,
        "Canvas search",
        hearthspace_common::canvas_search_app_id(
            search.snapshot.session,
            search.snapshot.output.as_deref(),
        ),
        LogicalSize::new(search.snapshot.field.width, search.snapshot.field.height),
        field::view(state),
        close,
    ));
    windows
}

pub(super) fn close(state: &mut ShellState) {
    let session = state.canvas_search.snapshot.session;
    submit_command(state, format!("canvas-search-close {session}"));
}

pub(super) fn activate(state: &mut ShellState, id: Option<&str>) {
    if !state.canvas_search.snapshot.active {
        return;
    }
    submit_command(
        state,
        format!(
            "canvas-search-activate {}",
            serde_json::json!({
                "session": state.canvas_search.snapshot.session, "id": id
            })
        ),
    );
}

fn change_query(state: &mut ShellState, mut text: String) {
    let end = text.floor_char_boundary(hearthspace_common::canvas_search::MAX_QUERY_BYTES);
    text.truncate(end);
    state.canvas_search.query = text;
    update(state);
}

fn update(state: &mut ShellState) {
    let search = &mut state.canvas_search;
    let edit = worker::Edit {
        session: search.snapshot.session,
        query: search.query.clone(),
        handwriting: false,
    };
    search.pending_edit = Some(edit.clone());
    if let Some(sender) = &search.edits {
        sender.send_replace(Some(edit));
    }
}

fn apply_snapshot(state: &mut ShellState, snapshot: Snapshot) {
    let search = &mut state.canvas_search;
    let new_session = search.snapshot.session != snapshot.session;
    if new_session || !snapshot.active {
        search.pending_edit = None;
        if let Some(edits) = &search.edits {
            edits.send_replace(None);
        }
    }
    if new_session && snapshot.active {
        search.window_id = WindowId::next();
    }
    if search.pending_edit.as_ref().is_some_and(|edit| {
        edit.session == snapshot.session
            && edit.query == snapshot.query
            && edit.handwriting == snapshot.handwriting
    }) {
        search.pending_edit = None;
    }
    if search.pending_edit.is_none() {
        search.query = snapshot.query.clone();
        search.handwriting_open = snapshot.handwriting;
    }
    search.snapshot = snapshot;
    search.error = None;
    if new_session && search.snapshot.active {
        state.dismiss_panels();
    }
}
