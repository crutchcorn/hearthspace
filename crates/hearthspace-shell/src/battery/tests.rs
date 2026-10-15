use super::{BatteryKind, BatteryState, BatteryWarning, model, policy::Warnings};

fn properties(percentage: f64, state: u32, warning: u32) -> model::Properties {
    [
        ("IsPresent".into(), true.into()),
        ("Type".into(), 2_u32.into()),
        ("Percentage".into(), percentage.into()),
        ("State".into(), state.into()),
        ("WarningLevel".into(), warning.into()),
    ]
    .into()
}

#[test]
fn no_battery_or_peripheral_is_not_a_system_supply() {
    let mut props = properties(80.0, 1, 1);
    props.insert("IsPresent".into(), false.into());
    assert!(model::decode(&props, false).is_none());
    props.insert("IsPresent".into(), true.into());
    for kind in [0_u32, 1, 5, 6, 8, 99] {
        props.insert("Type".into(), kind.into());
        assert!(model::decode(&props, false).is_none());
    }
}

#[test]
fn invalid_and_missing_percentage_are_not_reported_as_empty() {
    for percentage in [f64::NAN, f64::INFINITY, -1.0, 100.1] {
        assert!(model::decode(&properties(percentage, 2, 0), true).is_none());
    }
    let mut props = properties(0.0, 2, 0);
    assert_eq!(model::decode(&props, true).unwrap().percentage, 0);
    props.remove("Percentage");
    assert!(model::decode(&props, true).is_none());
}

#[test]
fn presentation_handles_each_upower_state_and_ups() {
    for (code, expected) in [
        (0, BatteryState::Unknown),
        (1, BatteryState::Charging),
        (2, BatteryState::Discharging),
        (3, BatteryState::Empty),
        (4, BatteryState::Full),
        (5, BatteryState::PendingCharge),
        (6, BatteryState::PendingDischarge),
        (99, BatteryState::Unknown),
    ] {
        let status = model::decode(&properties(75.6, code, 1), false).unwrap();
        assert_eq!(status.percentage, 76);
        assert_eq!(status.state, expected);
        assert!(!status.state_label().is_empty());
        assert_eq!(status.is_charging(), code == 1);
    }
    let mut props = properties(89.0, 4, 1);
    props.insert("Type".into(), 3_u32.into());
    let status = model::decode(&props, false).unwrap();
    assert_eq!(status.kind, BatteryKind::Ups);
    assert_eq!(status.label(), "89%");
    assert_eq!(status.accessible_label(), "UPS: 89%, Fully charged");
}

#[test]
fn charging_and_ac_never_warn_even_with_stale_warning_level() {
    for state in 0..=6 {
        assert!(
            model::decode(&properties(2.0, state, 5), false)
                .unwrap()
                .warning
                .is_none()
        );
    }
    for state in [1, 4, 5] {
        assert!(
            model::decode(&properties(2.0, state, 5), true)
                .unwrap()
                .warning
                .is_none()
        );
    }
}

#[test]
fn daemon_thresholds_are_authoritative_with_unknown_only_fallback() {
    for (percentage, level, warning) in [
        (50.0, 3, Some(BatteryWarning::Low)),
        (40.0, 4, Some(BatteryWarning::Critical)),
        (40.0, 5, Some(BatteryWarning::Critical)),
        (1.0, 1, None),
        (1.0, 2, None),
        (20.0, 0, Some(BatteryWarning::Low)),
        (20.01, 0, None),
        (5.0, 0, Some(BatteryWarning::Critical)),
        (5.01, 0, Some(BatteryWarning::Low)),
    ] {
        assert_eq!(
            model::decode(&properties(percentage, 2, level), true)
                .unwrap()
                .warning,
            warning
        );
    }
}

#[test]
fn deduplicates_reconnects_and_escalates_critical_once() {
    let mut warnings = Warnings::default();
    let low = model::decode(&properties(15.0, 2, 3), true);
    assert_eq!(warnings.pending(low), Some(BatteryWarning::Low));
    // Failed delivery leaves the warning eligible for a bounded retry.
    assert_eq!(warnings.pending(low), Some(BatteryWarning::Low));
    warnings.delivered(BatteryWarning::Low);
    assert_eq!(warnings.pending(low), None);
    assert_eq!(warnings.pending(None), None);
    assert_eq!(warnings.pending(low), None);
    let critical = model::decode(&properties(4.0, 2, 4), true);
    assert_eq!(warnings.pending(critical), Some(BatteryWarning::Critical));
    warnings.delivered(BatteryWarning::Critical);
    assert_eq!(warnings.pending(critical), None);
    assert_eq!(warnings.pending(low), None);
}

#[test]
fn charge_reset_and_hysteresis_rearm_without_threshold_chatter() {
    let mut warnings = Warnings::default();
    let low = model::decode(&properties(20.0, 2, 0), true);
    warnings.delivered(BatteryWarning::Low);
    assert_eq!(
        warnings.pending(model::decode(&properties(21.0, 2, 0), true)),
        None
    );
    assert_eq!(warnings.pending(low), None);
    warnings.pending(model::decode(&properties(25.0, 2, 0), true));
    assert_eq!(warnings.pending(low), Some(BatteryWarning::Low));
    warnings.delivered(BatteryWarning::Critical);
    warnings.pending(model::decode(&properties(6.0, 2, 0), true));
    let critical = model::decode(&properties(5.0, 2, 0), true);
    assert_eq!(warnings.pending(critical), None);
    warnings.pending(model::decode(&properties(10.0, 2, 0), true));
    assert_eq!(warnings.pending(critical), Some(BatteryWarning::Critical));
    warnings.delivered(BatteryWarning::Critical);
    warnings.pending(model::decode(&properties(5.0, 1, 4), false));
    assert_eq!(warnings.pending(critical), Some(BatteryWarning::Critical));
}
