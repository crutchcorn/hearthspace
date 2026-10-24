//! Native frame parts stay alive while the compositor owns their placement.
use std::{collections::BTreeMap, sync::Arc};

use hearthspace_common::frame_layout::LayoutTarget;
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
mod vertical_text;
mod widgets;
mod worker;

pub(super) fn task()
-> impl xilem::core::View<ShellState, (), xilem::ViewCtx, Element = xilem::core::NoElement> + use<>
{
    xilem::core::fork(task::task(), tooltip::task())
}

use hearthspace_ipc::frame_layout::{Action, Command};
#[cfg(test)]
pub(super) use hearthspace_ipc::frame_layout::{Confirmation, Group, Selection, Widget};
pub(super) use hearthspace_ipc::frame_layout::{
    FocusRequest, Output, PinnedWindow, Rect, Snapshot,
};

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

pub(super) fn command(state: &mut ShellState, action: Action) {
    let output = state
        .frame_layout
        .snapshot
        .selected
        .as_ref()
        .map(|selected| selected.output.clone())
        .or_else(|| state.frame_layout.snapshot.output.clone());
    if let Some(output) = output {
        command_on_output(state, output, action);
    }
}

pub(super) fn command_on_output(state: &mut ShellState, output: String, action: Action) {
    let request = Command {
        session: state.frame_layout.snapshot.session,
        output,
        action,
    };
    submit(
        state,
        format!(
            "layout-command {}",
            serde_json::to_string(&request).expect("serializable frame action")
        ),
    );
}

pub(super) fn selected_command(
    state: &mut ShellState,
    action: impl FnOnce(LayoutTarget) -> Action,
) {
    if let Some(selected) = &state.frame_layout.snapshot.selected {
        command(state, action(selected.target.clone()));
    }
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
