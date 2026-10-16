//! Small deterministic policy for a touch that began on shell chrome.
use std::time::{Duration, Instant};

use smithay::utils::{Logical, Point};

type Position = Point<f64, Logical>;

#[derive(Debug, PartialEq)]
pub(super) enum BarAction {
    Tap,
    OpenSearch,
    Cancel,
}

pub(super) struct BarSwipe {
    origin: Position,
    position: Position,
    moved: f64,
    started: Instant,
    cancelled: bool,
}

impl BarSwipe {
    pub(super) fn new(origin: Position, now: Instant) -> Self {
        Self {
            origin,
            position: origin,
            moved: 0.0,
            started: now,
            cancelled: false,
        }
    }

    pub(super) fn motion(&mut self, position: Position) {
        if !position.x.is_finite() || !position.y.is_finite() {
            self.cancelled = true;
            return;
        }
        self.position = position;
        let delta = position - self.origin;
        self.moved = self.moved.max(delta.x.hypot(delta.y));
        // Once a clearly different gesture was chosen, returning downward does
        // not turn that cancelled gesture into an activation.
        self.cancelled |=
            delta.y < -16.0 || (delta.x.abs() > 28.0 && delta.x.abs() > delta.y * 1.5);
    }

    pub(super) fn finish(&self, now: Instant) -> BarAction {
        if self.cancelled || now.duration_since(self.started) > Duration::from_secs(2) {
            return BarAction::Cancel;
        }
        let delta = self.position - self.origin;
        if delta.y >= 40.0 && delta.y >= delta.x.abs() * 1.5 {
            BarAction::OpenSearch
        } else if self.moved < 8.0 {
            BarAction::Tap
        } else {
            BarAction::Cancel
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taps_and_downward_swipes_have_distinct_thresholds() {
        let now = Instant::now();
        let mut swipe = BarSwipe::new((40.0, 8.0).into(), now);
        swipe.motion((43.0, 11.0).into());
        assert_eq!(swipe.finish(now), BarAction::Tap);
        swipe.motion((43.0, 30.0).into());
        assert_eq!(swipe.finish(now), BarAction::Cancel);
        swipe.motion((49.0, 50.0).into());
        assert_eq!(swipe.finish(now), BarAction::OpenSearch);
    }

    #[test]
    fn unrelated_or_abandoned_motion_never_becomes_a_tap_or_search() {
        let now = Instant::now();
        for wrong in [(80.0, 10.0), (40.0, -30.0), (f64::NAN, 60.0)] {
            let mut swipe = BarSwipe::new((40.0, 8.0).into(), now);
            swipe.motion(wrong.into());
            swipe.motion((40.0, 60.0).into());
            assert_eq!(swipe.finish(now), BarAction::Cancel);
        }
        let mut swipe = BarSwipe::new((40.0, 8.0).into(), now);
        swipe.motion((40.0, 70.0).into());
        assert_eq!(
            swipe.finish(now + Duration::from_secs(3)),
            BarAction::Cancel
        );
    }
}
