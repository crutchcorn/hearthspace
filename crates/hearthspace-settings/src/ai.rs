//! UI state for optional local decisions. All model work runs in `ai_worker`.

use hearthspace_ai::{DevicePreference, InstallPhase, InstallProgress, ModelStatus};
use xilem::tokio::sync::mpsc::UnboundedSender;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Refresh,
    Install,
    Remove,
    Warm,
    SetDevice(DevicePreference),
}

#[derive(Debug)]
pub(crate) enum Response {
    Progress(InstallProgress),
    Finished {
        operation: Operation,
        snapshot: Option<ModelStatus>,
        error: Option<String>,
    },
}

pub(crate) struct AiSettings {
    pub(crate) sender: Option<UnboundedSender<Operation>>,
    pub(crate) snapshot: Option<ModelStatus>,
    pub(crate) pending: Option<Operation>,
    pub(crate) progress: Option<f64>,
    pub(crate) message: String,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            sender: None,
            snapshot: None,
            pending: None,
            progress: None,
            message: "Checking local AI availability…".into(),
        }
    }
}

impl AiSettings {
    pub(crate) fn request(&mut self, operation: Operation) {
        if self.pending.is_some() {
            return;
        }
        let Some(sender) = &self.sender else {
            self.message = "Local AI settings are starting…".into();
            return;
        };
        if sender.send(operation).is_err() {
            self.message = "Local AI worker stopped. Reopen Settings to try again.".into();
            return;
        }
        self.pending = Some(operation);
        self.progress = None;
        self.message = match operation {
            Operation::Refresh => "Checking local model and Ollama…",
            Operation::Install => "Preparing the D1 model download…",
            Operation::Remove => "Deleting the D1 model and its download…",
            Operation::Warm => "Loading D1 into memory…",
            Operation::SetDevice(_) => "Saving processing mode and preparing D1…",
        }
        .into();
    }

    pub(crate) fn receive(&mut self, response: Response) {
        match response {
            Response::Progress(progress) if self.pending == Some(Operation::Install) => {
                self.progress = match progress.phase {
                    InstallPhase::Downloading => progress
                        .total
                        .filter(|&total| total > 0)
                        .map(|total| (progress.completed as f64 / total as f64).clamp(0.0, 1.0)),
                    _ => None,
                };
                self.message = match progress.phase {
                    InstallPhase::Downloading => match progress.total {
                        Some(total) if total > 0 => format!(
                            "Downloading D1: {:.0} / {:.0} MB",
                            progress.completed as f64 / 1_000_000.0,
                            total as f64 / 1_000_000.0,
                        ),
                        _ => format!(
                            "Downloading D1: {:.0} MB",
                            progress.completed as f64 / 1_000_000.0,
                        ),
                    },
                    InstallPhase::Importing => "Registering D1 with Ollama…".into(),
                    InstallPhase::Warming => "Loading D1 into memory…".into(),
                };
            }
            Response::Progress(_) => {}
            Response::Finished {
                operation,
                snapshot,
                error,
            } => {
                self.pending = None;
                self.progress = None;
                if let Some(snapshot) = snapshot {
                    self.message = match operation {
                        _ if snapshot.runtime_error.is_some() => {
                            snapshot.runtime_error.as_ref().unwrap().clone()
                        }
                        Operation::Remove => "D1 model deleted.".into(),
                        _ if snapshot.installed && snapshot.warm => {
                            "D1 is ready to gather similar windows.".into()
                        }
                        _ if snapshot.installed => {
                            "D1 is installed. It will load when needed.".into()
                        }
                        _ => "D1 is optional and is not installed.".into(),
                    };
                    self.snapshot = Some(snapshot);
                }
                if let Some(error) = error {
                    self.message = format!("Local AI unavailable: {error}");
                }
            }
        }
    }

    pub(crate) fn installed(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|status| status.installed)
    }

    pub(crate) fn device(&self) -> DevicePreference {
        self.snapshot
            .as_ref()
            .map_or(DevicePreference::Automatic, |status| status.device)
    }

    pub(crate) fn can_remove(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|status| status.installed || status.downloaded)
    }
}

#[cfg(test)]
mod tests;
