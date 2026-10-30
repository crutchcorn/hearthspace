use std::collections::HashSet;

/// A lock acknowledgement is a presentation barrier, not a rendering or
/// submission barrier. Tags prevent an older, already queued normal frame from
/// satisfying a lock requested before that frame's page-flip arrives.
#[derive(Default)]
pub(super) struct PresentationBarrier {
    generation: u64,
    active: bool,
    outputs_unavailable: bool,
    outputs: HashSet<String>,
    presented: HashSet<String>,
}

impl PresentationBarrier {
    pub(super) fn begin(&mut self, outputs: impl IntoIterator<Item = String>) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.active = true;
        self.outputs = outputs.into_iter().collect();
        self.presented.clear();
        self.generation
    }

    pub(super) fn generation(&self) -> Option<u64> {
        self.active.then_some(self.generation)
    }

    /// KMS state can be lost across a seat handoff or system sleep. Preserve
    /// ownership/isolation, but require fresh flips from the rebuilt surfaces.
    pub(super) fn invalidate(&mut self) {
        if self.active {
            self.generation = self.generation.wrapping_add(1);
            self.presented.clear();
        }
    }

    pub(super) fn update_outputs(&mut self, outputs: impl IntoIterator<Item = String>) {
        self.outputs = outputs.into_iter().collect();
        self.presented.retain(|name| self.outputs.contains(name));
    }

    /// Published outputs may omit a connected target whose GPU failed to open.
    /// An incomplete backend inventory cannot certify a locked desktop.
    #[cfg(any(feature = "udev", test))]
    pub(super) fn set_outputs_available(&mut self, available: bool) {
        if !available && !self.outputs_unavailable {
            self.invalidate();
        }
        self.outputs_unavailable = !available;
    }

    pub(super) fn presented(&mut self, name: &str, generation: u64) {
        if self.active && self.generation == generation && self.outputs.contains(name) {
            self.presented.insert(name.to_string());
        }
    }

    pub(super) fn ready(&self) -> bool {
        self.active && !self.outputs_unavailable && self.outputs.is_subset(&self.presented)
    }

    pub(super) fn unlock(&mut self) {
        self.active = false;
        self.outputs.clear();
        self.presented.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_output_must_present_the_current_lock_generation() {
        let mut barrier = PresentationBarrier::default();
        let first = barrier.begin(["internal".into(), "external".into()]);
        barrier.presented("internal", first);
        assert!(!barrier.ready());
        barrier.presented("external", first.wrapping_sub(1));
        assert!(
            !barrier.ready(),
            "a pre-lock page flip is not a locked frame"
        );
        barrier.presented("external", first);
        assert!(barrier.ready());
        barrier.unlock();
        let second = barrier.begin(["internal".into()]);
        barrier.presented("internal", first);
        assert!(!barrier.ready(), "a previous lock's delayed event is stale");
        barrier.presented("internal", second);
        assert!(barrier.ready());
    }

    #[test]
    fn resume_requires_new_frames_without_releasing_the_lock() {
        let mut barrier = PresentationBarrier::default();
        barrier.invalidate();
        assert_eq!(barrier.generation(), None);
        let original = barrier.begin(["internal".into(), "external".into()]);
        barrier.presented("internal", original);
        barrier.presented("external", original);
        assert!(barrier.ready());
        barrier.invalidate();
        let resumed = barrier.generation().expect("resume must keep isolation");
        assert_ne!(original, resumed);
        assert!(!barrier.ready());
        barrier.presented("internal", original);
        barrier.presented("external", original);
        assert!(!barrier.ready(), "pre-suspend flips are stale");
        barrier.presented("internal", resumed);
        assert!(!barrier.ready());
        barrier.presented("external", resumed);
        assert!(barrier.ready());
    }

    #[test]
    fn hotplug_joins_barrier_and_removed_outputs_stop_blocking_it() {
        let mut barrier = PresentationBarrier::default();
        let generation = barrier.begin(["internal".into()]);
        barrier.presented("internal", generation);
        barrier.update_outputs(["internal".into(), "external".into()]);
        assert!(!barrier.ready());
        barrier.update_outputs(["internal".into()]);
        assert!(barrier.ready());
        barrier.update_outputs(["external".into()]);
        assert!(
            !barrier.ready(),
            "new output must present even after old output leaves"
        );
    }

    #[test]
    fn no_outputs_needs_no_page_flip_and_reconnection_requires_a_new_one() {
        let mut barrier = PresentationBarrier::default();
        let generation = barrier.begin([]);
        assert!(barrier.ready());
        barrier.update_outputs(["external".into()]);
        assert!(!barrier.ready());
        barrier.presented("external", generation);
        assert!(barrier.ready());
        barrier.update_outputs([]);
        assert!(barrier.ready());
        barrier.update_outputs(["external".into()]);
        assert!(
            !barrier.ready(),
            "a reconnected output has no retained presentation"
        );
    }

    #[test]
    fn failed_gpu_cannot_acknowledge_a_subset_or_empty_output_inventory() {
        let mut barrier = PresentationBarrier::default();
        barrier.set_outputs_available(false);
        let first = barrier.begin([]);
        assert!(
            !barrier.ready(),
            "unknown outputs are not disconnected outputs"
        );
        barrier.update_outputs(["external".into()]);
        barrier.presented("external", first);
        assert!(
            !barrier.ready(),
            "a healthy GPU does not prove the other GPU is shielded"
        );
        barrier.update_outputs(["external".into(), "internal".into()]);
        barrier.set_outputs_available(true);
        assert!(
            !barrier.ready(),
            "recovered output must still present the shield"
        );
        barrier.presented("internal", first);
        assert!(barrier.ready());

        barrier.set_outputs_available(false);
        let recovered = barrier.generation().unwrap();
        assert_ne!(first, recovered);
        barrier.set_outputs_available(true);
        barrier.presented("internal", first);
        barrier.presented("external", first);
        assert!(
            !barrier.ready(),
            "pre-failure presentations cannot acknowledge recovery"
        );
        barrier.presented("internal", recovered);
        barrier.presented("external", recovered);
        assert!(barrier.ready());
    }
}
