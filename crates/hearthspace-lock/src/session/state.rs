use crate::auth::{Completion, Failure};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Phase {
    #[default]
    Acquiring,
    Locked,
    UnlockRequested,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Effect {
    None,
    Feedback(Failure),
    Unlock,
}

/// The sole authority to request unlock. Outputs and window focus deliberately
/// have no authority here: unplugging every monitor cannot end the session.
#[derive(Default)]
pub(super) struct State {
    phase: Phase,
    generation: u64,
    attempt: Option<u64>,
    authenticated: bool,
}

impl State {
    pub(super) fn begin_authentication(&mut self) -> Option<u64> {
        if !self.accepts_outputs() || self.busy() {
            return None;
        }
        self.generation = self.generation.checked_add(1)?;
        self.attempt = Some(self.generation);
        Some(self.generation)
    }

    pub(super) fn authentication_finished(&mut self, completion: Completion) -> Effect {
        if !self.accepts_outputs() || self.attempt != Some(completion.generation) {
            return Effect::None;
        }
        self.attempt = None;
        match completion.result {
            Ok(()) => {
                self.authenticated = true;
                self.unlock_if_ready()
            }
            Err(failure) => Effect::Feedback(failure),
        }
    }

    pub(super) fn locked(&mut self) -> Effect {
        if self.phase == Phase::Acquiring {
            self.phase = Phase::Locked;
        }
        self.unlock_if_ready()
    }

    fn unlock_if_ready(&mut self) -> Effect {
        if self.phase == Phase::Locked && self.authenticated {
            // Set this before calling the library: its unlocked signal is synchronous.
            self.phase = Phase::UnlockRequested;
            Effect::Unlock
        } else {
            Effect::None
        }
    }

    pub(super) fn ended(&mut self) {
        let authorized = self.phase == Phase::UnlockRequested;
        self.failed();
        if authorized {
            self.phase = Phase::Completed;
        }
    }

    pub(super) fn failed(&mut self) {
        self.phase = Phase::Failed;
        self.attempt = None;
        self.authenticated = false;
    }

    pub(super) fn busy(&self) -> bool {
        self.attempt.is_some() || self.authenticated || !self.accepts_outputs()
    }

    pub(super) fn accepts_outputs(&self) -> bool {
        matches!(self.phase, Phase::Acquiring | Phase::Locked)
    }

    pub(super) fn succeeded(&self) -> bool {
        self.phase == Phase::Completed
    }
}

#[cfg(test)]
mod tests;
