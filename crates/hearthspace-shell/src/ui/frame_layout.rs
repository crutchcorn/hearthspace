//! Native frame parts stay alive while the compositor owns their placement.
use std::{collections::BTreeMap, sync::Arc};

use hearthspace_common::frame_layout::{FrameLayout, LayoutTarget};
use serde::Deserialize;
use tokio::sync::{Notify, mpsc};
use xilem::{WindowId, WindowView};

use super::state::ShellState;

mod backing;
mod confirm;
mod dock;
pub(super) use dock::width as dock_width;
mod editor;
mod editor_clock;
pub(super) mod focus;
pub(super) mod items;
mod parts;
mod popup_intent;
pub(super) mod popups;
mod reveal;
mod task;
#[cfg(test)]
mod tests;
mod text_metrics;
pub(super) mod tool_menu;
mod tooltip;
mod tray;
mod widgets;
mod worker;

pub(super) fn task()
-> impl xilem::core::View<ShellState, (), xilem::ViewCtx, Element = xilem::core::NoElement> + use<>
{
    xilem::core::fork(task::task(), tooltip::task())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub(super) struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub(super) struct Widget {
    pub id: String,
    pub group: Option<String>,
    pub orientation: String,
    pub rect: Rect,
    pub expanded: bool,
    pub merged: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub(super) struct Group {
    pub id: String,
    pub rect: Rect,
    pub grip: Rect,
    #[serde(default = "group_scale")]
    pub scale: f64,
    pub orientation: String,
    pub hovered: bool,
    pub focused: bool,
    #[serde(default = "group_material")]
    pub material: bool,
    pub backing_only: bool,
}

fn group_scale() -> f64 {
    1.0
}

fn group_material() -> bool {
    true
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub(super) struct Output {
    pub output: String,
    pub rect: Rect,
    pub compact: bool,
    pub layout: FrameLayout,
    pub widgets: Vec<Widget>,
    pub groups: Vec<Group>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub(super) struct Selection {
    pub output: String,
    pub target: LayoutTarget,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub(super) struct FocusRequest {
    pub serial: u64,
    pub output: String,
    pub target: LayoutTarget,
    pub reverse: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub(super) struct PinnedWindow {
    pub id: u64,
    pub title: String,
    pub app_id: String,
    pub output: String,
    pub rect: Rect,
    pub expanded: bool,
    pub hovered: bool,
    pub edge: Option<hearthspace_common::frame_layout::FrameEdge>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub(super) struct Confirmation {
    pub kind: String,
    pub window_id: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub(super) struct Snapshot {
    pub version: u32,
    pub revision: u64,
    pub popup_epoch: u64,
    pub editing: bool,
    pub session: u64,
    pub output: Option<String>,
    pub selected: Option<Selection>,
    pub selected_title: String,
    pub confirmation: Option<Confirmation>,
    #[serde(default = "confirm_width")]
    pub confirmation_max_width: f64,
    #[serde(default = "editor_height")]
    pub confirmation_max_height: f64,
    pub outputs: Vec<Output>,
    pub pinned_windows: Vec<PinnedWindow>,
    pub focus_request: Option<FocusRequest>,
    #[serde(default = "editor_width")]
    pub editor_max_width: f64,
    #[serde(default = "editor_height")]
    pub editor_max_height: f64,
}

#[derive(Default)]
pub(super) struct State {
    pub snapshot: Snapshot,
    pub popup_epoch: u64,
    visible: bool,
    pub refresh: Arc<Notify>,
    requests: Option<mpsc::Sender<task::Request>>,
    opening_popup: bool,
    pub intents: popup_intent::Intents,
    availability: Option<tokio::sync::watch::Sender<Vec<(String, String, bool)>>>,
    windows: BTreeMap<String, WindowId>,
    chains: BTreeMap<String, focus::Chain>,
    editor: Option<WindowId>,
    tooltip: tooltip::State,
    confirm_window: Option<WindowId>,
    confirm_height: Option<f64>,
    confirm_target: super::search_focus::FocusTarget,
    tool_menu_target: super::search_focus::FocusTarget,
    pub error: Option<String>,
}

pub(super) fn submit(state: &mut ShellState, command: String) {
    if command == "layout-tool" {
        state.frame_layout.intents.cancel();
    }
    if let Some(requests) = &state.frame_layout.requests
        && let Err(error) = requests.try_send(task::Request {
            command,
            open: None,
        })
    {
        state.frame_layout.error = Some(format!("Layout request could not be queued: {error}"));
    }
}

pub(super) fn command(state: &mut ShellState, action: &str, data: serde_json::Value) {
    let snapshot = &state.frame_layout.snapshot;
    let output = snapshot
        .selected
        .as_ref()
        .map(|selected| selected.output.as_str())
        .or(snapshot.output.as_deref());
    let mut request = serde_json::json!({"session":snapshot.session,"output":output,
        "action":action,"target":snapshot.selected.as_ref().map(|value| &value.target)});
    if let (Some(request), Some(data)) = (request.as_object_mut(), data.as_object()) {
        request.extend(data.clone());
    }
    submit(state, format!("layout-command {request}"));
}

fn apply(state: &mut ShellState, mut snapshot: Snapshot) {
    if snapshot.popup_epoch < state.frame_layout.popup_epoch {
        return;
    }
    state.frame_layout.popup_epoch = snapshot.popup_epoch;
    state
        .frame_layout
        .intents
        .observe_epoch(snapshot.popup_epoch);
    // Empty outputs mean locked/inactive, before cached geometry is restored.
    if snapshot.outputs.is_empty() || snapshot.editing || snapshot.confirmation.is_some() {
        state.frame_layout.intents.cancel();
    } else {
        state
            .frame_layout
            .intents
            .retain_output(|key| snapshot.outputs.iter().any(|output| output.output == key));
    }
    state.frame_layout.visible = !snapshot.outputs.is_empty();
    if snapshot.outputs.is_empty() {
        snapshot.outputs = state.frame_layout.snapshot.outputs.clone();
    }
    let new_session = state.frame_layout.snapshot.session != snapshot.session;
    if new_session
        || !snapshot.editing
        || snapshot.selected != state.frame_layout.snapshot.selected
        || snapshot.confirmation != state.frame_layout.snapshot.confirmation
    {
        tooltip::clear(state);
    }
    if new_session || !snapshot.editing {
        state.frame_layout.editor = snapshot.editing.then(WindowId::next);
    }
    if new_session || snapshot.confirmation != state.frame_layout.snapshot.confirmation {
        state.frame_layout.confirm_window =
            snapshot.confirmation.as_ref().map(|_| WindowId::next());
        state.frame_layout.confirm_height = None;
        state.frame_layout.confirm_target = Default::default();
    }
    for pinned in &snapshot.pinned_windows {
        state
            .frame_layout
            .chains
            .entry(
                LayoutTarget::new(
                    hearthspace_common::frame_layout::TargetKind::Window,
                    pinned.id.to_string(),
                )
                .key(),
            )
            .or_default();
        state
            .frame_layout
            .windows
            .entry(format!("reveal:{}", pinned.id))
            .or_insert_with(WindowId::next);
    }
    for output in &snapshot.outputs {
        for id in hearthspace_common::frame_layout::WIDGETS {
            let key = widget_key(&output.output, id);
            state
                .frame_layout
                .windows
                .entry(key.clone())
                .or_insert_with(WindowId::next);
            state.frame_layout.chains.entry(key).or_default();
        }
        for target in output
            .layout
            .placements
            .keys()
            .filter_map(|key| LayoutTarget::from_key(key))
            .chain(
                output
                    .layout
                    .groups
                    .values()
                    .flat_map(|group| group.members.iter().cloned()),
            )
        {
            if target.kind == hearthspace_common::frame_layout::TargetKind::Item {
                state.frame_layout.chains.entry(target.key()).or_default();
            }
        }
        for group in &output.groups {
            for role in ["group", "grip"] {
                state
                    .frame_layout
                    .windows
                    .entry(format!("{role}:{}:{}", output.output, group.id))
                    .or_insert_with(WindowId::next);
            }
        }
    }
    if snapshot.editing && (!state.frame_layout.snapshot.editing || new_session) {
        state.dismiss_panels();
    }
    state.frame_layout.snapshot = snapshot;
}

pub(super) fn cancel_popups(state: &mut ShellState, epoch: u64) {
    if observe_popup_epoch(state, epoch) {
        state.dismiss_panels();
    }
}

pub(super) fn observe_popup_epoch(state: &mut ShellState, epoch: u64) -> bool {
    if epoch <= state.frame_layout.popup_epoch {
        return false;
    }
    state.frame_layout.intents.observe_epoch(epoch);
    state.frame_layout.popup_epoch = epoch;
    state.frame_layout.refresh.notify_one();
    true
}

fn widget_key(output: &str, id: &str) -> String {
    format!("widget:{output}:{id}")
}

pub(super) fn windows(state: &ShellState) -> Vec<WindowView<ShellState>> {
    let mut windows = parts::windows(state);
    windows.extend(backing::windows(state));
    windows.extend(editor::window(state));
    windows.extend(confirm::window(state));
    windows.extend(tooltip::window(state));
    windows.extend(reveal::windows(state));
    windows
}

fn material<V: xilem::WidgetView<ShellState>>(
    inner: V,
    visible: bool,
    state: &ShellState,
) -> Box<xilem::AnyWidgetView<ShellState>> {
    use xilem::WidgetView as _;
    if visible {
        super::canvas_search::material::shell(inner, state.appearance.palette).boxed()
    } else {
        inner.boxed()
    }
}

fn editor_width() -> f64 {
    260.0
}
fn editor_height() -> f64 {
    452.0
}

fn confirm_width() -> f64 {
    420.0
}

pub(super) fn reconcile(state: &ShellState) {
    if let Some(sender) = &state.frame_layout.availability {
        let available = state
            .frame_layout
            .snapshot
            .outputs
            .iter()
            .flat_map(|output| {
                [
                    (
                        output.output.clone(),
                        "battery".into(),
                        state.battery.is_some(),
                    ),
                    (
                        output.output.clone(),
                        "tray".into(),
                        super::tray_state::visible(&state.tray.snapshot)
                            .next()
                            .is_some(),
                    ),
                ]
            })
            .collect::<Vec<_>>();
        sender.send_if_modified(|previous| {
            if *previous == available {
                false
            } else {
                *previous = available;
                true
            }
        });
    }
}
