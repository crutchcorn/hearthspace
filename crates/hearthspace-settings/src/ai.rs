//! UI state for optional local decisions. All model work runs in `ai_worker`.

use hearthspace_ai::{InstallPhase, InstallProgress, ModelStatus};
use xilem::tokio::sync::mpsc::UnboundedSender;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Refresh,
    Install,
    EnableInstalled,
    Remove,
    Warm,
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
    pub(crate) features_open: [bool; 2],
    pub(crate) confirm_off: bool,
    pub(crate) failed_operation: Option<Operation>,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            sender: None,
            snapshot: None,
            pending: None,
            progress: None,
            message: "Checking local AI availability…".into(),
            features_open: [false; 2],
            confirm_off: false,
            failed_operation: None,
        }
    }
}

impl AiSettings {
    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        if self.pending.is_some() || self.confirm_off {
            return;
        }
        if enabled {
            self.request(self.enable_operation());
        } else if self.can_remove() {
            // The toggle alone never removes a model or its download.
            self.confirm_off = true;
        }
    }

    pub(crate) fn confirm_removal(&mut self) {
        if self.confirm_off && self.pending.is_none() {
            self.confirm_off = false;
            self.request(Operation::Remove);
        }
    }

    pub(crate) fn enable_operation(&self) -> Operation {
        if self.installed() {
            Operation::EnableInstalled
        } else {
            Operation::Install
        }
    }

    pub(crate) fn retry(&mut self) {
        match self.failed_operation {
            Some(Operation::Remove) => self.confirm_off = true,
            Some(Operation::Install) => self.request(self.enable_operation()),
            Some(operation) => self.request(operation),
            None => self.request(Operation::Refresh),
        }
    }

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
        self.failed_operation = None;
        self.progress = None;
        self.message = match operation {
            Operation::Refresh => "Checking AI availability…",
            Operation::Install => "Preparing the model download…",
            Operation::Remove => "Turning off AI and deleting the local model…",
            Operation::Warm | Operation::EnableInstalled => "Loading the local model…",
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
                            "Downloading model: {:.0} / {:.0} MB",
                            progress.completed as f64 / 1_000_000.0,
                            total as f64 / 1_000_000.0,
                        ),
                        _ => format!(
                            "Downloading model: {:.0} MB",
                            progress.completed as f64 / 1_000_000.0,
                        ),
                    },
                    InstallPhase::Importing => "Setting up the local model…".into(),
                    InstallPhase::Warming => "Loading the local model…".into(),
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
                self.failed_operation = error.as_ref().map(|_| operation);
                if let Some(snapshot) = snapshot {
                    self.message = if let Some(error) = &snapshot.runtime_error {
                        error.clone()
                    } else if snapshot.registered && snapshot.installed && snapshot.warm {
                        "AI features are ready.".into()
                    } else if snapshot.registered && snapshot.installed {
                        "AI is enabled. The local model is not ready yet.".into()
                    } else if snapshot.downloaded && !snapshot.installed {
                        "AI features are off. Model setup is incomplete.".into()
                    } else {
                        "AI features are off.".into()
                    };
                    self.snapshot = Some(snapshot);
                }
                if let Some(error) = error {
                    self.message = format!("Local AI unavailable: {error}");
                } else if operation == Operation::Refresh
                    && self.snapshot.as_ref().is_some_and(|snapshot| {
                        snapshot.registered
                            && snapshot.installed
                            && !snapshot.warm
                            && snapshot.runtime_error.is_none()
                    })
                {
                    // An existing enabled model can be cold after a session restart.
                    // Renew its existing opt-in; never download during a refresh.
                    self.request(Operation::Warm);
                }
            }
        }
    }

    pub(crate) fn installed(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|status| status.installed)
    }

    pub(crate) fn enabled(&self) -> bool {
        matches!(
            self.pending,
            Some(Operation::Install | Operation::EnableInstalled | Operation::Warm)
        ) || self
            .snapshot
            .as_ref()
            .is_some_and(|status| status.registered && status.installed)
    }

    pub(crate) fn needs_retry(&self) -> bool {
        self.failed_operation.is_some()
            || self.snapshot.as_ref().is_some_and(|status| {
                status.runtime_error.is_some()
                    || (status.registered && status.installed && !status.warm)
            })
    }

    pub(crate) fn can_remove(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|status| status.registered || status.installed || status.downloaded)
    }
}

#[cfg(test)]
mod tests;
