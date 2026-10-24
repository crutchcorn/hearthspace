//! UI-independent observations of connected Wayland outputs, never preferences.
use std::io;
use wayland_client::Connection;

mod observer;
mod state;

pub use observer::{connect_wayland, watch_outputs};

/// Values are published together at wl_output.done (or immediately for version 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputInfo {
    pub global_name: u32,
    pub name: Option<String>,
    pub description: Option<String>,
    pub position: Option<(i32, i32)>,
    /// Physical pixels of the advertised current mode; no fallback mode is invented.
    pub size: Option<(i32, i32)>,
    pub refresh_millihertz: Option<i32>,
    pub scale: i32,
}

impl OutputInfo {
    pub fn connector(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("output-{}", self.global_name))
    }
}

/// Synchronous bootstrap for callers that already run outside their UI loop.
/// Continuous clients should use `connect_wayland` and `watch_outputs` instead.
pub fn initial_outputs() -> io::Result<Vec<OutputInfo>> {
    let connection = Connection::connect_to_env().map_err(io::Error::other)?;
    let mut queue = connection.new_event_queue();
    let _registry = connection.display().get_registry(&queue.handle(), ());
    let mut watcher = state::OutputWatcher::default();
    // First bind registry outputs, then receive their initial state.
    for _ in 0..2 {
        queue.roundtrip(&mut watcher).map_err(io::Error::other)?;
    }
    Ok(watcher.snapshot())
}
