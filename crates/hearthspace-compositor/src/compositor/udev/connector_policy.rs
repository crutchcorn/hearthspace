//! Target comparison separated from device access for deterministic lifecycle tests.
pub(super) fn targets_need_rebuild<T: PartialEq>(
    current: &[T],
    discovered: &[T],
    complete_surface_set: bool,
) -> bool {
    current != discovered || (!discovered.is_empty() && !complete_surface_set)
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
}
