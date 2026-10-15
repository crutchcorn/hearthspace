use std::collections::HashMap;
use zbus::zvariant::OwnedValue;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryKind {
    Battery,
    Ups,
}

impl BatteryKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Battery => "Battery",
            Self::Ups => "UPS",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryState {
    Unknown,
    Charging,
    Discharging,
    Empty,
    Full,
    PendingCharge,
    PendingDischarge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryWarning {
    Low,
    Critical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatteryStatus {
    pub percentage: u8,
    pub state: BatteryState,
    pub kind: BatteryKind,
    pub on_battery: bool,
    pub warning: Option<BatteryWarning>,
}

impl BatteryStatus {
    pub fn state_label(&self) -> &'static str {
        match self.state {
            BatteryState::Unknown => "Status unknown",
            BatteryState::Charging => "Charging",
            BatteryState::Discharging => "Discharging",
            BatteryState::Empty => "Empty",
            BatteryState::Full => "Fully charged",
            BatteryState::PendingCharge => "Plugged in, not charging",
            BatteryState::PendingDischarge => "Waiting to discharge",
        }
    }

    pub fn is_charging(&self) -> bool {
        self.state == BatteryState::Charging
    }

    pub fn label(&self) -> String {
        format!("{}%", self.percentage)
    }

    pub fn accessible_label(&self) -> String {
        format!(
            "{}: {}%, {}",
            self.kind.label(),
            self.percentage,
            self.state_label()
        )
    }
}

pub(super) type Properties = HashMap<String, OwnedValue>;

/// DisplayDevice is UPower's aggregate, not an enumerated physical device. Its
/// documented fields deliberately do not require the optional PowerSupply flag.
pub(super) fn decode(properties: &Properties, on_battery: bool) -> Option<BatteryStatus> {
    if !bool::try_from(properties.get("IsPresent")?).ok()? {
        return None;
    }
    let kind = match u32::try_from(properties.get("Type")?).ok()? {
        2 => BatteryKind::Battery,
        3 => BatteryKind::Ups,
        _ => return None,
    };
    let raw_percentage = f64::try_from(properties.get("Percentage")?).ok()?;
    if !raw_percentage.is_finite() || !(0.0..=100.0).contains(&raw_percentage) {
        return None;
    }
    let state = match u32::try_from(properties.get("State")?).ok()? {
        1 => BatteryState::Charging,
        2 => BatteryState::Discharging,
        3 => BatteryState::Empty,
        4 => BatteryState::Full,
        5 => BatteryState::PendingCharge,
        6 => BatteryState::PendingDischarge,
        _ => BatteryState::Unknown,
    };
    let warning = properties
        .get("WarningLevel")
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    let warning = if !on_battery
        || matches!(
            state,
            BatteryState::Charging | BatteryState::Full | BatteryState::PendingCharge
        ) {
        None
    } else {
        match warning {
            3 => Some(BatteryWarning::Low),
            4 | 5 => Some(BatteryWarning::Critical),
            1 | 2 => None, // Honor the daemon's configured thresholds.
            _ if raw_percentage <= 5.0 => Some(BatteryWarning::Critical),
            _ if raw_percentage <= 20.0 => Some(BatteryWarning::Low),
            _ => None,
        }
    };
    Some(BatteryStatus {
        percentage: raw_percentage.round() as u8,
        kind,
        state,
        on_battery,
        warning,
    })
}
