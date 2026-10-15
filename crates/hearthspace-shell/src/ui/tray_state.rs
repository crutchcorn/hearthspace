//! UI-only tray state. All application operations are queued to the bus worker.
use super::state::ShellState;
use crate::tray::{
    ItemKey, TrayCommand, TrayHandle, TrayItem, TrayMenuItem, TraySnapshot, TrayStatus,
};
use xilem::WindowId;

#[derive(Default)]
pub(super) struct TrayState {
    pub snapshot: TraySnapshot,
    pub handle: Option<TrayHandle>,
    pub hovered: Option<(ItemKey, String)>,
}

#[derive(Debug)]
pub(super) struct TrayOutput {
    pub window_id: WindowId,
    pub open: bool,
    pub item: Option<ItemKey>,
    pub menu_path: Option<String>,
    pub path: Vec<i32>,
    pub selected: Option<i32>,
    pub bar_width: f64,
    pub error: Option<String>,
}
impl Default for TrayOutput {
    fn default() -> Self {
        Self {
            window_id: WindowId::next(),
            open: false,
            item: None,
            menu_path: None,
            path: Vec::new(),
            selected: None,
            bar_width: 0.0,
            error: None,
        }
    }
}
impl TrayOutput {
    pub fn dismiss(&mut self) {
        self.open = false;
        self.item = None;
        self.menu_path = None;
        self.path.clear();
        self.selected = None;
        self.error = None;
    }
}

pub(super) fn visible(snapshot: &TraySnapshot) -> impl Iterator<Item = &TrayItem> {
    snapshot
        .items
        .iter()
        .filter(|item| item.status != TrayStatus::Passive)
}

pub(super) fn capacity(width: f64) -> usize {
    // Keep the clock and desktop controls available; the drawer fits even on 320px outputs.
    ((width - 330.0).max(0.0) / 38.0).floor().min(8.0) as usize
}

pub(super) fn menu_items<'a>(
    snapshot: &'a TraySnapshot,
    output: &TrayOutput,
) -> &'a [TrayMenuItem] {
    let Some(item) = output
        .item
        .as_ref()
        .and_then(|key| visible(snapshot).find(|item| &item.key == key))
    else {
        return &[];
    };
    if item.menu_path != output.menu_path {
        return &[];
    }
    let Some(menu) = &item.menu else { return &[] };
    let mut nodes = menu.items.as_slice();
    for id in &output.path {
        let Some(node) = nodes
            .iter()
            .find(|node| node.id == *id && node.visible && node.enabled)
        else {
            return &[];
        };
        nodes = &node.children;
    }
    nodes
}

fn valid_path(snapshot: &TraySnapshot, output: &TrayOutput) -> bool {
    let Some(item) = output
        .item
        .as_ref()
        .and_then(|key| visible(snapshot).find(|item| &item.key == key))
    else {
        return false;
    };
    if item.menu_path != output.menu_path {
        return false;
    }
    let Some(menu) = &item.menu else { return false };
    let mut nodes = menu.items.as_slice();
    for id in &output.path {
        let Some(node) = nodes.iter().find(|node| {
            node.id == *id && actionable(node) && (node.submenu || !node.children.is_empty())
        }) else {
            return false;
        };
        nodes = &node.children;
    }
    true
}

pub(super) fn reconcile(state: &mut ShellState) {
    if state
        .tray
        .hovered
        .as_ref()
        .is_some_and(|(key, _)| !visible(&state.tray.snapshot).any(|item| &item.key == key))
    {
        state.tray.hovered = None;
    }
    for output in &mut state.outputs {
        if output.tray.item.as_ref().is_some_and(|key| {
            !visible(&state.tray.snapshot)
                .any(|item| &item.key == key && item.menu_path == output.tray.menu_path)
        }) || (output.tray.open && visible(&state.tray.snapshot).next().is_none())
        {
            output.tray.dismiss();
            continue;
        }
        // A live menu can remove the open submenu. Return to the nearest valid ancestor.
        while !output.tray.path.is_empty() && !valid_path(&state.tray.snapshot, &output.tray) {
            output.tray.path.pop();
        }
        let nodes = menu_items(&state.tray.snapshot, &output.tray);
        if !nodes
            .iter()
            .any(|node| Some(node.id) == output.tray.selected && actionable(node))
        {
            output.tray.selected = nodes
                .iter()
                .find(|node| actionable(node))
                .map(|node| node.id);
        }
    }
}

fn actionable(node: &TrayMenuItem) -> bool {
    node.visible && node.enabled && !node.separator
}

pub(super) fn send(state: &mut ShellState, output: &str, command: TrayCommand) {
    let error = state
        .tray
        .handle
        .as_ref()
        .ok_or_else(|| "Application tray is unavailable".to_string())
        .and_then(|handle| handle.try_send(command))
        .err();
    if let Some(output) = state.outputs.iter_mut().find(|o| o.key() == output) {
        output.tray.error = error;
    }
}

pub(super) fn toggle(state: &mut ShellState, key: &str) {
    let open = state
        .outputs
        .iter()
        .any(|output| output.key() == key && !output.tray.open);
    state.dismiss_panels();
    if open && let Some(output) = state.outputs.iter_mut().find(|output| output.key() == key) {
        output.tray.open = true;
    }
}

pub(super) fn dismiss(state: &mut ShellState, key: &str) {
    if let Some(output) = state.outputs.iter_mut().find(|output| output.key() == key) {
        output.tray.dismiss();
    }
}

pub(super) fn open_menu(state: &mut ShellState, output_key: &str, key: &ItemKey) {
    let Some(item) = visible(&state.tray.snapshot).find(|item| &item.key == key) else {
        return;
    };
    if item.menu.is_none() || item.menu_path.is_none() {
        send(
            state,
            output_key,
            TrayCommand::Context {
                key: key.clone(),
                x: 0,
                y: 0,
            },
        );
        dismiss(state, output_key);
        return;
    }
    let menu_path = item.menu_path.clone().expect("menu path checked above");
    state.dismiss_panels();
    if let Some(output) = state
        .outputs
        .iter_mut()
        .find(|output| output.key() == output_key)
    {
        output.tray.open = true;
        output.tray.item = Some(key.clone());
        output.tray.menu_path = Some(menu_path.clone());
    }
    send(
        state,
        output_key,
        TrayCommand::OpenMenu {
            key: key.clone(),
            menu_path,
            id: 0,
        },
    );
    reconcile(state);
}

pub(super) fn activate(state: &mut ShellState, output: &str, key: &ItemKey) {
    let Some(item) = visible(&state.tray.snapshot).find(|item| &item.key == key) else {
        return;
    };
    if item.item_is_menu {
        open_menu(state, output, key);
    } else {
        send(
            state,
            output,
            TrayCommand::Activate {
                key: key.clone(),
                x: 0,
                y: 0,
            },
        );
    }
}

pub(super) fn choose(state: &mut ShellState, output_key: &str, id: i32) {
    let Some(output) = state
        .outputs
        .iter_mut()
        .find(|output| output.key() == output_key)
    else {
        return;
    };
    let Some(key) = output.tray.item.clone() else {
        return;
    };
    let Some(menu_path) = output.tray.menu_path.clone() else {
        return;
    };
    let Some(node) = menu_items(&state.tray.snapshot, &output.tray)
        .iter()
        .find(|node| node.id == id && actionable(node))
    else {
        return;
    };
    if node.submenu || !node.children.is_empty() {
        output.tray.path.push(id);
        output.tray.selected = None;
        send(
            state,
            output_key,
            TrayCommand::OpenMenu { key, menu_path, id },
        );
        reconcile(state);
    } else {
        send(
            state,
            output_key,
            TrayCommand::MenuClick { key, menu_path, id },
        );
        dismiss(state, output_key);
    }
}

pub(super) fn step(state: &mut ShellState, output_key: &str, direction: i32) {
    let Some(output) = state
        .outputs
        .iter_mut()
        .find(|output| output.key() == output_key)
    else {
        return;
    };
    let nodes = menu_items(&state.tray.snapshot, &output.tray);
    let ids = nodes
        .iter()
        .filter(|node| actionable(node))
        .map(|node| node.id)
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return;
    }
    let current = ids
        .iter()
        .position(|id| Some(*id) == output.tray.selected)
        .unwrap_or(0) as i32;
    output.tray.selected = Some(ids[(current + direction).rem_euclid(ids.len() as i32) as usize]);
}

pub(super) fn back(state: &mut ShellState, output_key: &str) {
    if let Some(output) = state
        .outputs
        .iter_mut()
        .find(|output| output.key() == output_key)
    {
        if let Some(id) = output.tray.path.pop() {
            output.tray.selected = Some(id);
        } else {
            output.tray.item = None;
            output.tray.menu_path = None;
            output.tray.selected = None;
        }
    }
}
