//! Target comparison separated from device access for deterministic lifecycle tests.
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
