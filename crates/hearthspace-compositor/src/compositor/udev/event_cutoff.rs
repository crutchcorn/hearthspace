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
        }
    }

    pub(super) fn accepts(&self, metadata: Option<&DrmEventMetadata>) -> bool {
        match metadata.map(|metadata| metadata.time) {
            Some(DrmEventTime::Monotonic(time)) => time.as_micros() >= self.monotonic_micros,
            Some(DrmEventTime::Realtime(time)) => system_time_micros(time) >= self.realtime_micros,
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
}
