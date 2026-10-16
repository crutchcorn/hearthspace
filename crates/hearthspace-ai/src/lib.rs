//! Local, text-only D1 decisions and optional model management.
//!
//! All public operations are blocking: run them on a dedicated worker, never the
//! compositor, UI, or async runtime thread. Window titles only go to loopback
//! Ollama; Hugging Face is contacted solely for an explicit model install.

mod decision;
mod download;
mod http;
mod model;
mod model_files;

pub use anyhow::{Error, Result};
pub use decision::{DecisionClient, WindowCandidate};
pub use model::{DevicePreference, InstallPhase, InstallProgress, ModelManager, ModelStatus};

pub const MODEL_NAME: &str = "hearthspace-d1:latest";
pub const MODEL_FILE: &str = "d1-3B-Q4_K_M.gguf";
pub const MODEL_REPOSITORY: &str = "LiquidAI/d1-3B-GGUF";

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod tests;
