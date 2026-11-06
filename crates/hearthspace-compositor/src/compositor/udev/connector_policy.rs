//! Target comparison separated from device access for deterministic lifecycle tests.
use std::time::Instant;

/// Give only this device's new configuration time to reach its first flip.
/// Pending scenes are deliberately not an exemption once the deadline passes.
pub(super) fn refresh_after_configuration<T: PartialEq>(
    current: &[T],
    discovered: &[T],
    complete_surface_set: bool,
    force_rebuild: bool,
    now: Instant,
    not_before: Option<Instant>,
    rebuild: impl FnOnce() -> bool,
) -> TargetRefresh {
    if current == discovered && not_before.is_some_and(|deadline| now < deadline) {
        return TargetRefresh {
            rebuilt: false,
            recovery_needed: force_rebuild,
        };
    }
    refresh_targets(
        current,
        discovered,
        complete_surface_set,
        force_rebuild,
        rebuild,
    )
}

pub(super) fn targets_need_rebuild<T: PartialEq>(
    current: &[T],
    discovered: &[T],
    complete_surface_set: bool,
) -> bool {
    current != discovered || (!discovered.is_empty() && !complete_surface_set)
}

pub(super) struct TargetRefresh {
    pub(super) rebuilt: bool,
    pub(super) recovery_needed: bool,
}

/// Reconcile usable targets and scanout failures only. Discovery completeness
/// independently controls backend recovery/readiness, never surface replacement.
/// The callback is the external KMS surface rebuild and source invalidation.
pub(super) fn refresh_targets<T: PartialEq>(
    current: &[T],
    discovered: &[T],
    complete_surface_set: bool,
    force_rebuild: bool,
    rebuild: impl FnOnce() -> bool,
) -> TargetRefresh {
    let rebuilt = force_rebuild || targets_need_rebuild(current, discovered, complete_surface_set);
    let rebuild_failed = rebuilt && !rebuild();
    TargetRefresh {
        rebuilt,
        recovery_needed: rebuild_failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unplugging_last_connector_and_replugging_a_new_one_rebuilds_scanout() {
        let internal = ["eDP-1"];
        let external = ["DP-1"];
        let none: [&str; 0] = [];
        assert!(targets_need_rebuild(&internal, &none, true));
        assert!(!targets_need_rebuild(&none, &none, false));
        assert!(targets_need_rebuild(&none, &external, false));
        assert!(!targets_need_rebuild(&external, &external, true));
    }

    #[test]
    fn unchanged_targets_retry_failed_surface_creation() {
        let outputs = ["eDP-1", "DP-1"];
        assert!(targets_need_rebuild(&outputs, &outputs, false));
        assert!(!targets_need_rebuild(&outputs, &outputs, true));
    }

    #[test]
    fn repeated_discovery_of_unchanged_usable_targets_preserves_scanout() {
        let usable = ["eDP-1"];
        let healthy = refresh_targets(&usable, &usable, true, false, || {
            panic!("unchanged healthy targets must not rebuild")
        });
        assert!(!healthy.rebuilt);
        assert!(!healthy.recovery_needed);

        // A newly connected DP-1 has no usable mode/CRTC. The scanner keeps
        // eDP-1 as its only target. Discovery retries must not become scanout
        // failures on the next attempt, even though global readiness is false.
        let mut scanout_repair = healthy.recovery_needed;
        for _ in 0..3 {
            let retry = refresh_targets(&usable, &usable, true, scanout_repair, || {
                panic!("incomplete discovery must preserve healthy scanout")
            });
            assert!(!retry.rebuilt);
            assert!(!retry.recovery_needed);
            scanout_repair = retry.recovery_needed;
        }

        let mut rebuilt = false;
        let recovered = refresh_targets(&usable, &["eDP-1", "DP-1"], false, scanout_repair, || {
            rebuilt = true;
            true
        });
        assert!(rebuilt && recovered.rebuilt);
        assert!(!recovered.recovery_needed);
    }

    #[test]
    fn actual_scanout_failure_or_unpresented_retry_still_rebuilds() {
        let outputs = ["eDP-1"];
        // This force flag also covers a paused device or a missing first flip.
        let failed = refresh_targets(&outputs, &outputs, true, true, || false);
        assert!(failed.rebuilt && failed.recovery_needed);
        let repaired = refresh_targets(&outputs, &outputs, true, failed.recovery_needed, || true);
        assert!(repaired.rebuilt);
        assert!(!repaired.recovery_needed);
    }
}

#[cfg(test)]
mod asynchronous_recovery_tests {
    use super::*;
    use crate::compositor::udev::{
        identity::DeviceToken, presentation::OutputPresentation, presenter::OutputKey,
        recovery::CONFIRMATION_GRACE,
    };
    use std::{num::NonZeroU32, time::Duration};

    #[test]
    fn missing_first_flip_rebuilds_despite_pending_scene_and_busy_healthy_output() {
        let started = Instant::now();
        let output = |id| OutputKey {
            device: DeviceToken { id, incarnation: 1 },
            crtc: NonZeroU32::new(7).unwrap().into(),
            generation: 1,
        };
        let stuck_key = output(1);
        let healthy_key = output(2);
        let mut stuck = OutputPresentation::new(stuck_key);
        let mut healthy = OutputPresentation::new(healthy_key);
        stuck.queued(1);
        healthy.queued(1);
        assert_eq!(healthy.presented(healthy_key, 1, 1), Some(true));
        let targets = ["DP-1"];
        let mut rebuilds = 0;
        for tick in 1..=5 {
            let now = started + Duration::from_millis(tick * 100);
            assert_eq!(healthy.presented(healthy_key, tick + 1, 1), Some(false));
            let result = refresh_after_configuration(
                &targets,
                &targets,
                true,
                !stuck.presented_once,
                now,
                Some(started + CONFIRMATION_GRACE),
                || {
                    rebuilds += 1;
                    true
                },
            );
            assert_eq!(result.rebuilt, tick == 5);
            assert!(stuck.pending());
            let healthy_result = refresh_after_configuration(
                &targets,
                &targets,
                true,
                !healthy.presented_once,
                now,
                Some(started + CONFIRMATION_GRACE),
                || panic!("healthy output must retain its configuration"),
            );
            assert!(!healthy_result.rebuilt);
        }
        assert_eq!(rebuilds, 1);
    }

    #[test]
    fn connector_replacement_bypasses_old_configuration_grace() {
        let now = Instant::now();
        let mut replaced = false;
        let refresh = refresh_after_configuration(
            &["DP-1"],
            &["DP-2"],
            false,
            true,
            now,
            Some(now + CONFIRMATION_GRACE),
            || {
                replaced = true;
                true
            },
        );
        assert!(refresh.rebuilt && replaced);
    }
}
