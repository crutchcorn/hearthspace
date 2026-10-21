//! Small events invalidate state; bounded snapshots recover dropped datagrams.
use std::collections::BTreeMap;

use serde::Deserialize;

use super::{ShellState, task};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum Event {
    Changed,
    Action(u64),
}

pub(in crate::ui) fn parse_event(value: &serde_json::Value) -> Option<Event> {
    if value["version"] != 1 {
        return None;
    }
    match value["action"].as_str()? {
        "desktop-changed" => Some(Event::Changed),
        "desktop-action" => value["token"]
            .as_u64()
            .filter(|token| *token > 0)
            .map(Event::Action),
        _ => None,
    }
}

pub(in crate::ui) fn apply_event(state: &mut ShellState, event: Event) {
    match event {
        Event::Changed => state.desktop.refresh.notify_one(),
        Event::Action(token) => task::submit(state, task::Request::Accept(token)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub revision: u64,
    pub select_mode: bool,
    pub items: BTreeMap<String, Flags>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            select_mode: true,
            items: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct Flags {
    pub selected: bool,
    pub focused: bool,
    pub keyboard_focus: bool,
}

pub(super) fn parse_snapshot(bytes: &[u8]) -> Result<Snapshot, String> {
    #[derive(Deserialize)]
    struct WireItem {
        id: String,
        #[serde(flatten)]
        flags: Flags,
    }
    #[derive(Deserialize)]
    struct Wire {
        version: u32,
        revision: u64,
        select_mode: bool,
        items: Vec<WireItem>,
    }
    let wire: Wire = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid desktop status: {error}"))?;
    if wire.version != 1 {
        return Err("Invalid desktop status version".into());
    }
    let mut snapshot = Snapshot {
        revision: wire.revision,
        select_mode: wire.select_mode,
        items: BTreeMap::new(),
    };
    for item in wire.items {
        if item.id.is_empty()
            || item.id.chars().any(char::is_control)
            || snapshot.items.insert(item.id, item.flags).is_some()
        {
            return Err("Invalid desktop item identity".into());
        }
    }
    Ok(snapshot)
}
