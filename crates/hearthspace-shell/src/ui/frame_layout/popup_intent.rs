//! One cancellable opening shared by queued work, the worker, and UI delivery.
use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use super::{Snapshot, popups::Popup};
use crate::ui::{popup::PopupKind, state::ShellState};

#[derive(Clone, Debug)]
pub(super) struct Intent {
    pub key: String,
    pub popup: Popup,
    epoch: u64,
    current: Arc<AtomicBool>,
    expires: Arc<OnceLock<Instant>>,
}

impl Intent {
    pub fn is_current(&self) -> bool {
        self.valid_at(Instant::now())
    }

    fn valid_at(&self, now: Instant) -> bool {
        if self.expires.get().is_some_and(|expires| now >= *expires) {
            self.cancel();
        }
        self.current.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        self.current.store(false, Ordering::Release);
    }

    pub fn acknowledge(self, diagnostic_id: u64, now: Instant) -> Opening {
        self.expires.get_or_init(|| now + Duration::from_secs(5));
        Opening {
            intent: self,
            diagnostic_id,
        }
    }
}

#[derive(Default)]
pub(in crate::ui) struct Intents(Option<Intent>);

impl Intents {
    pub(super) fn begin(&mut self, key: String, popup: Popup, epoch: u64) -> Intent {
        self.cancel();
        let intent = Intent {
            key,
            popup,
            epoch,
            current: Arc::new(AtomicBool::new(true)),
            expires: Arc::new(OnceLock::new()),
        };
        self.0 = Some(intent.clone());
        intent
    }

    pub(super) fn reverse(&mut self, key: &str, popup: &Popup, epoch: u64) -> Option<Popup> {
        if self.0.as_ref().is_some_and(|intent| {
            intent.is_current()
                && intent.epoch == epoch
                && intent.key == key
                && popup.toggles()
                && intent.popup.same_destination(popup)
        }) {
            let intent = self.0.take().unwrap();
            intent.cancel();
            Some(intent.popup)
        } else {
            None
        }
    }

    pub(super) fn observe_epoch(&mut self, epoch: u64) {
        if self.0.as_ref().is_some_and(|intent| intent.epoch != epoch) {
            self.cancel();
        }
    }

    pub(in crate::ui) fn cancel(&mut self) {
        if let Some(intent) = self.0.take() {
            intent.cancel();
        }
    }

    pub(in crate::ui) fn cancel_output(&mut self, key: &str) {
        if self.0.as_ref().is_some_and(|intent| intent.key == key) {
            self.cancel();
        }
    }

    pub(in crate::ui) fn cancel_kind(&mut self, key: &str, kind: PopupKind) {
        if self
            .0
            .as_ref()
            .is_some_and(|intent| intent.key == key && intent.popup.matches(kind))
        {
            self.cancel();
        }
    }

    pub(in crate::ui) fn retain_output(&mut self, present: impl FnOnce(&str) -> bool) {
        if self.0.as_ref().is_some_and(|intent| !present(&intent.key)) {
            self.cancel();
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Opening {
    pub intent: Intent,
    pub diagnostic_id: u64,
}

impl Opening {
    pub fn valid_at(&self, now: Instant) -> bool {
        self.intent.valid_at(now)
    }

    pub fn eligible(&self, snapshot: &Snapshot) -> bool {
        snapshot.popup_epoch == self.intent.epoch
            && !snapshot.editing
            && snapshot.confirmation.is_none()
            && snapshot
                .outputs
                .iter()
                .any(|output| output.output == self.intent.key)
    }

    pub fn deliver(self, state: &mut ShellState, now: Instant) {
        let eligible = self.valid_at(now)
            && state.frame_layout.visible
            && state.frame_layout.popup_epoch == self.intent.epoch
            && self.eligible(&state.frame_layout.snapshot)
            && state
                .outputs
                .iter()
                .any(|output| output.key() == self.intent.key);
        tracing::debug!(target: "hearthspace_shell::ui::frame_layout::task", diagnostic_id = self.diagnostic_id, eligible,
            "frame layout popup update received");
        // A cloned/queued update can consume this intent at most once.
        if eligible && self.intent.current.swap(false, Ordering::AcqRel) {
            super::popups::open(state, &self.intent.key, self.intent.popup);
        } else {
            self.intent.cancel();
        }
    }
}

#[cfg(test)]
#[path = "popup_intent_tests.rs"]
mod tests;
