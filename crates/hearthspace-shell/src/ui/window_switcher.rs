//! Passive title for the compositor's live Alt+Tab window previews.
use super::state::ShellState;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Notify;
use xilem::{WindowId, WindowView, dpi::LogicalSize};

#[path = "window_switcher/caption.rs"]
mod caption;
#[path = "window_switcher/text.rs"]
mod text;
#[path = "window_switcher/task.rs"]
mod worker;
pub(super) use worker::task;
#[cfg(test)]
#[path = "window_switcher/tests.rs"]
mod tests;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
struct Snapshot {
    version: u32,
    revision: u64,
    active: bool,
    session: u64,
    title: String,
    index: usize,
    count: usize,
    #[serde(deserialize_with = "optional_bounds")]
    caption: Bounds,
}

fn optional_bounds<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Bounds, D::Error> {
    Ok(Option::<Bounds>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
struct Bounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    max_width: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Prepared {
    snapshot: Snapshot,
    displayed: String,
}

pub(super) struct State {
    prepared: Prepared,
    window_id: WindowId,
    pub refresh: Arc<Notify>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            prepared: Prepared::default(),
            window_id: WindowId::next(),
            refresh: Arc::default(),
        }
    }
}

pub(super) fn window(state: &ShellState) -> Option<WindowView<ShellState>> {
    let switcher = &state.window_switcher;
    let snapshot = &switcher.prepared.snapshot;
    snapshot.active.then(|| {
        super::popup::window(
            switcher.window_id,
            "Window switcher",
            hearthspace_common::window_switcher_caption_app_id(snapshot.session),
            LogicalSize::new(snapshot.caption.width, 24.0),
            caption::view(state),
            |_| {},
        )
    })
}

fn apply(state: &mut ShellState, prepared: Prepared) {
    let previous = &state.window_switcher.prepared.snapshot;
    if prepared.snapshot.active
        && (!previous.active || previous.session != prepared.snapshot.session)
    {
        // Old buffers cannot display another session's private window title.
        state.window_switcher.window_id = WindowId::next();
    }
    state.window_switcher.prepared = prepared;
}
