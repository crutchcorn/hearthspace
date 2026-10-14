use std::collections::HashSet;

/// A lock acknowledgement is a presentation barrier, not a rendering or
/// submission barrier. Tags prevent an older, already queued normal frame from
/// satisfying a lock requested before that frame's page-flip arrives.
#[derive(Default)]
pub(super) struct PresentationBarrier {
    generation: u64,
    active: bool,
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

    pub(super) fn update_outputs(&mut self, outputs: impl IntoIterator<Item = String>) {
        self.outputs = outputs.into_iter().collect();
        self.presented.retain(|name| self.outputs.contains(name));
    }

    pub(super) fn presented(&mut self, name: &str, generation: u64) {
        if self.active && self.generation == generation && self.outputs.contains(name) {
            self.presented.insert(name.to_string());
        }
    }

    pub(super) fn ready(&self) -> bool {
        self.active && self.outputs.is_subset(&self.presented)
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
}
