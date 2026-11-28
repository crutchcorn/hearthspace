//! Native display-mode catalog and compositor-owned preview transactions.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mode {
    /// Identity of the complete DRM timing, not just its rounded refresh rate.
    pub id: String,
    pub width: i32,
    pub height: i32,
    pub refresh_millihertz: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Output {
    pub id: String,
    pub current: Mode,
    pub modes: Vec<Mode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    pub token: u64,
    pub output: String,
    pub ready: bool,
    pub seconds_remaining: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub available: bool,
    pub outputs: Vec<Output>,
    pub preview: Option<Preview>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Request {
    Preview { output: String, mode: String },
    Keep { token: u64 },
    Revert { token: u64 },
}
