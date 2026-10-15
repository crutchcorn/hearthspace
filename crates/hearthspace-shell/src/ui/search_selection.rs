//! Stable search selection survives live metadata refreshes and clamps on removal.
use super::{
    launcher::{self, Entry},
    state::{ShellOutputInstance, ShellState},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EntryId {
    Window(u64),
    App(String),
}

pub(super) fn selected_index(output: &ShellOutputInstance, entries: &[Entry]) -> Option<usize> {
    if entries.is_empty() {
        return None;
    }
    Some(
        output
            .selected_result
            .as_ref()
            .and_then(|id| entries.iter().position(|entry| &entry.id() == id))
            .unwrap_or(output.selected_result_index.min(entries.len() - 1)),
    )
}

pub(super) fn reconcile(state: &mut ShellState) {
    let selections = state
        .outputs
        .iter()
        .map(|output| {
            let entries = launcher::results(state, output);
            selected_index(output, &entries).map(|index| (index, entries[index].id()))
        })
        .collect::<Vec<_>>();
    for (output, selected) in state.outputs.iter_mut().zip(selections) {
        output.selected_result_index = selected.as_ref().map_or(0, |(index, _)| *index);
        output.selected_result = selected.map(|(_, id)| id);
    }
}

pub(super) fn step(state: &mut ShellState, key: &str, delta: i32) {
    let Some(output) = state.outputs.iter().find(|output| output.key() == key) else {
        return;
    };
    let entries = launcher::results(state, output);
    let Some(index) = selected_index(output, &entries) else {
        return;
    };
    let index = index
        .saturating_add_signed(delta as isize)
        .min(entries.len() - 1);
    let output = state
        .outputs
        .iter_mut()
        .find(|output| output.key() == key)
        .unwrap();
    output.selected_result = Some(entries[index].id());
    output.selected_result_index = index;
}

pub(super) fn choose(state: &mut ShellState, key: &str, id: &EntryId) {
    let Some(output) = state.outputs.iter().find(|output| output.key() == key) else {
        return;
    };
    let entries = launcher::results(state, output);
    let Some(index) = entries.iter().position(|entry| &entry.id() == id) else {
        return;
    };
    let output = state
        .outputs
        .iter_mut()
        .find(|output| output.key() == key)
        .unwrap();
    output.selected_result_index = index;
    output.selected_result = Some(id.clone());
}

pub(super) fn activate(state: &mut ShellState, key: &str) {
    let Some(output) = state.outputs.iter().find(|output| output.key() == key) else {
        return;
    };
    let entries = launcher::results(state, output);
    let Some(index) = selected_index(output, &entries) else {
        return;
    };
    let output_name = output.output_name.clone();
    launcher::select(state, key, output_name.as_deref(), &entries[index]);
}

#[cfg(test)]
#[path = "search_selection_tests.rs"]
mod tests;
