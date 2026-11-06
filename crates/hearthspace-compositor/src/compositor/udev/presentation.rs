//! Main-side association of worker completions with produced scenes.
//! The worker validates DRM timestamps and acknowledges KMS buffers itself.
use super::presenter::OutputKey;

pub(super) struct OutputPresentation {
    key: OutputKey,
    pending: Option<u64>,
    newest_scene: u64,
    last_submission: u64,
    last_scene: u64,
    pub(super) dirty: bool,
    pub(super) presented_once: bool,
}

impl OutputPresentation {
    pub(super) fn new(key: OutputKey) -> Self {
        Self {
            key,
            pending: None,
            newest_scene: 0,
            last_submission: 0,
            last_scene: 0,
            dirty: true,
            presented_once: false,
        }
    }

    pub(super) fn pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn needs_scene(&self) -> bool {
        self.dirty && !self.pending()
    }

    /// New App damage applies to all outputs. Merely retrying an already dirty
    /// output after its completion does not invent damage on the other outputs.
    pub(super) fn request_scene(&mut self, changed: bool) -> bool {
        self.dirty |= changed;
        self.needs_scene()
    }

    pub(super) fn queued(&mut self, sequence: u64) {
        self.newest_scene = sequence;
        self.pending = Some(sequence);
        self.dirty = false;
    }

    pub(super) fn invalidate(&mut self) {
        self.pending = None;
        self.last_scene = 0;
        self.dirty = true;
        self.presented_once = false;
    }

    pub(super) fn discarded(&mut self, key: OutputKey, scene: u64) -> bool {
        if key != self.key || self.pending != Some(scene) {
            return false;
        }
        self.pending = None;
        self.dirty = true;
        true
    }

    /// A cursor-only completion may service cursor callbacks, but cannot retire
    /// a newer produced scene or release that scene's capture request.
    pub(super) fn presented(
        &mut self,
        key: OutputKey,
        submission: u64,
        scene: u64,
    ) -> Option<bool> {
        if key != self.key
            || submission <= self.last_submission
            || scene > self.newest_scene
            || scene < self.last_scene
        {
            return None;
        }
        self.last_submission = submission;
        self.last_scene = scene;
        self.presented_once = true;
        let completed = self.pending == Some(scene);
        if completed {
            self.pending = None;
        }
        Some(completed)
    }
}

#[cfg(test)]
#[path = "presentation_tests.rs"]
mod tests;
