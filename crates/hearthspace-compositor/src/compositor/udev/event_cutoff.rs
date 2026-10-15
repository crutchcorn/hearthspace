use std::time::{Duration, SystemTime};

use smithay::{
    backend::drm::{DrmEventMetadata, DrmEventTime},
    reexports::rustix::time::Timespec,
    utils::{Clock, Monotonic, Realtime},
};

/// Rejects queued events from an earlier surface that used the same CRTC.
///
/// DRM event timestamps have microsecond precision, so equality must be accepted.
/// Realtime-only devices also inherit the kernel clock's wall-clock adjustments;
/// this boundary is not a substitute for a kernel submission identity.
#[derive(Clone, Copy)]
pub(super) struct SurfaceEventCutoff {
    monotonic_micros: u128,
    realtime_micros: i128,
    untimed_retry_available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SurfaceEvent {
    Current,
    Untimed,
    Stale,
}

impl SurfaceEventCutoff {
    pub(super) fn now() -> Self {
        Self::from_clocks(
            Clock::<Monotonic>::new().now().into(),
            Clock::<Realtime>::new().now().into(),
        )
    }

    fn from_clocks(monotonic: Duration, realtime: Timespec) -> Self {
        Self {
            monotonic_micros: monotonic.as_micros(),
            realtime_micros: i128::from(realtime.tv_sec) * 1_000_000
                + i128::from(realtime.tv_nsec) / 1_000,
            untimed_retry_available: true,
        }
    }

    /// Classify only when a frame is pending. AMD's first full modeset after
    /// powering an output on can complete with both count and timestamp zero.
    /// Retiring that buffer permits one normal flip with trustworthy timing;
    /// the untimed event itself never establishes presentation or lock readiness.
    /// See Mesa commit b4a1756fd1f37b28858ba4c871188c1bf2b6f4f3.
    pub(super) fn classify(&mut self, metadata: Option<&DrmEventMetadata>) -> SurfaceEvent {
        if self.accepts(metadata) {
            self.untimed_retry_available = false;
            return SurfaceEvent::Current;
        }
        let untimed = metadata.is_some_and(|metadata| {
            metadata.sequence == 0
                && match metadata.time {
                    DrmEventTime::Monotonic(time) => time.is_zero(),
                    DrmEventTime::Realtime(time) => time == SystemTime::UNIX_EPOCH,
                }
        });
        if untimed && std::mem::take(&mut self.untimed_retry_available) {
            SurfaceEvent::Untimed
        } else {
            SurfaceEvent::Stale
        }
    }

    fn accepts(&self, metadata: Option<&DrmEventMetadata>) -> bool {
        match metadata.map(|metadata| metadata.time) {
            Some(DrmEventTime::Monotonic(time)) => {
                !time.is_zero() && time.as_micros() >= self.monotonic_micros
            }
            Some(DrmEventTime::Realtime(time)) => {
                time != SystemTime::UNIX_EPOCH && system_time_micros(time) >= self.realtime_micros
            }
            None => false,
        }
    }
}

fn system_time_micros(time: SystemTime) -> i128 {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => duration.as_micros() as i128,
        // Floor a negative timestamp just as CLOCK_REALTIME's timespec does.
        Err(error) => -(error.duration().as_nanos().div_ceil(1_000) as i128),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cutoff() -> SurfaceEventCutoff {
        SurfaceEventCutoff::from_clocks(
            Duration::new(10, 123_456_789),
            Timespec {
                tv_sec: 1_000,
                tv_nsec: 123_456_789,
            },
        )
    }

    fn metadata(time: DrmEventTime) -> DrmEventMetadata {
        DrmEventMetadata { time, sequence: 7 }
    }

    #[test]
    fn rejects_old_surface_event_and_accepts_current_monotonic_event() {
        let cutoff = cutoff();
        assert!(
            !cutoff.accepts(Some(&metadata(DrmEventTime::Monotonic(Duration::new(
                10,
                123_455_000,
            )))))
        );
        assert!(
            cutoff.accepts(Some(&metadata(DrmEventTime::Monotonic(Duration::new(
                10,
                123_457_000,
            )))))
        );
    }

    #[test]
    fn rejects_old_surface_event_and_accepts_current_realtime_event() {
        let cutoff = cutoff();
        let time = SystemTime::UNIX_EPOCH + Duration::new(1_000, 123_455_000);
        assert!(!cutoff.accepts(Some(&metadata(DrmEventTime::Realtime(time)))));
        assert!(cutoff.accepts(Some(&metadata(DrmEventTime::Realtime(
            time + Duration::from_micros(2),
        )))));
    }

    #[test]
    fn compares_in_kernel_microseconds_without_rejecting_equal_timestamps() {
        let cutoff = cutoff();
        assert!(!cutoff.accepts(None));
        assert!(
            cutoff.accepts(Some(&metadata(DrmEventTime::Monotonic(Duration::new(
                10,
                123_456_000,
            )))))
        );
        assert!(cutoff.accepts(Some(&metadata(DrmEventTime::Realtime(
            SystemTime::UNIX_EPOCH + Duration::new(1_000, 123_456_000),
        )))));
    }

    fn untimed(time: DrmEventTime) -> DrmEventMetadata {
        DrmEventMetadata { time, sequence: 0 }
    }

    #[test]
    fn first_untimed_modeset_allows_exactly_one_unverified_retry() {
        for time in [
            DrmEventTime::Monotonic(Duration::ZERO),
            DrmEventTime::Realtime(SystemTime::UNIX_EPOCH),
        ] {
            let mut cutoff = cutoff();
            let event = untimed(time);
            assert_eq!(cutoff.classify(Some(&event)), SurfaceEvent::Untimed);
            assert_eq!(cutoff.classify(Some(&event)), SurfaceEvent::Stale);
            assert_eq!(
                cutoff.classify(Some(&metadata(DrmEventTime::Monotonic(Duration::new(
                    11, 0,
                ))))),
                SurfaceEvent::Current
            );
        }
    }

    #[test]
    fn missing_metadata_and_old_timestamps_never_authorize_an_untimed_retry() {
        let mut cutoff = cutoff();
        assert_eq!(cutoff.classify(None), SurfaceEvent::Stale);
        assert_eq!(
            cutoff.classify(Some(&untimed(DrmEventTime::Monotonic(Duration::new(9, 0))))),
            SurfaceEvent::Stale
        );
        assert_eq!(
            cutoff.classify(Some(&metadata(DrmEventTime::Monotonic(Duration::ZERO)))),
            SurfaceEvent::Stale
        );
        // Ignoring unrelated old events does not consume the one modeset retry.
        assert_eq!(
            cutoff.classify(Some(&untimed(DrmEventTime::Monotonic(Duration::ZERO)))),
            SurfaceEvent::Untimed
        );
    }

    #[test]
    fn a_current_timestamp_is_valid_even_when_the_sequence_wraps_to_zero() {
        let mut cutoff = cutoff();
        assert_eq!(
            cutoff.classify(Some(&untimed(DrmEventTime::Monotonic(Duration::new(
                11, 0
            ))))),
            SurfaceEvent::Current
        );
        assert_eq!(
            cutoff.classify(Some(&untimed(DrmEventTime::Monotonic(Duration::ZERO)))),
            SurfaceEvent::Stale
        );
    }

    #[test]
    fn a_rebuilt_surface_gets_a_new_bounded_untimed_retry() {
        let event = untimed(DrmEventTime::Monotonic(Duration::ZERO));
        let mut first = cutoff();
        assert_eq!(first.classify(Some(&event)), SurfaceEvent::Untimed);
        assert_eq!(first.classify(Some(&event)), SurfaceEvent::Stale);
        assert_eq!(cutoff().classify(Some(&event)), SurfaceEvent::Untimed);
    }
}
