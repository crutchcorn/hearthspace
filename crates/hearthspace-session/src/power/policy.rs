//! Lid handling is edge-triggered: resume and session activation never re-sleep.

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LidState {
    pub closed: bool,
    pub active: bool,
    pub docked: bool,
    pub external_display: bool,
    pub controls_lid: bool,
}

#[derive(Default)]
pub(crate) struct LidPolicy {
    last_closed: Option<bool>,
    was_controlling: bool,
}

impl LidPolicy {
    /// Return true once for an eligible, observed open-to-closed transition.
    /// Call even while inactive so another desktop's close is not replayed.
    pub(crate) fn update(&mut self, state: LidState) -> bool {
        let closing = self.last_closed == Some(false) && state.closed && self.was_controlling;
        self.last_closed = Some(state.closed);
        self.was_controlling = state.active && state.controls_lid;
        closing && state.active && state.controls_lid && !state.docked && !state.external_display
    }

    /// Forget observations across bus loss; a new connection needs a baseline.
    pub(crate) fn reset(&mut self) {
        self.last_closed = None;
        self.was_controlling = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active(closed: bool) -> LidState {
        LidState {
            closed,
            active: true,
            controls_lid: true,
            ..LidState::default()
        }
    }

    #[test]
    fn closes_once_and_rearms_only_after_opening() {
        let mut policy = LidPolicy::default();
        assert!(!policy.update(active(false)));
        assert!(policy.update(active(true)));
        assert!(
            !policy.update(active(true)),
            "no loop after wake or failed suspend"
        );
        assert!(!policy.update(active(false)));
        assert!(policy.update(active(true)));
    }

    #[test]
    fn startup_and_reconnection_with_closed_lid_never_suspend() {
        let mut policy = LidPolicy::default();
        assert!(!policy.update(active(true)));
        assert!(!policy.update(active(false)));
        policy.reset();
        assert!(!policy.update(active(true)));
    }

    #[test]
    fn ignored_closure_is_not_replayed_when_policy_becomes_eligible() {
        for blocked in [
            LidState {
                active: false,
                ..active(true)
            },
            LidState {
                controls_lid: false,
                ..active(true)
            },
            LidState {
                docked: true,
                ..active(true)
            },
            LidState {
                external_display: true,
                ..active(true)
            },
        ] {
            let mut policy = LidPolicy::default();
            assert!(!policy.update(active(false)));
            assert!(!policy.update(blocked));
            assert!(!policy.update(active(true)));
            assert!(!policy.update(active(false)));
            assert!(policy.update(active(true)));
        }
    }

    #[test]
    fn activation_cannot_claim_a_simultaneous_closure() {
        let mut policy = LidPolicy::default();
        assert!(!policy.update(LidState {
            active: false,
            ..active(false)
        }));
        assert!(!policy.update(active(true)));
        assert!(!policy.update(active(false)));
        assert!(policy.update(active(true)));
    }
}
