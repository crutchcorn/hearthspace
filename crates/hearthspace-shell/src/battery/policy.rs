use super::{BatteryState, BatteryStatus, BatteryWarning};

/// Retained across transient UPower disconnects, so reconnecting at the same
/// charge cannot repeatedly notify. Rearm only on meaningful recovery or AC.
#[derive(Default)]
pub(super) struct Warnings {
    low_sent: bool,
    critical_sent: bool,
}

impl Warnings {
    pub fn pending(&mut self, status: Option<BatteryStatus>) -> Option<BatteryWarning> {
        let status = status?;
        if !status.on_battery
            || matches!(
                status.state,
                BatteryState::Charging | BatteryState::Full | BatteryState::PendingCharge
            )
        {
            *self = Self::default();
            return None;
        }
        if status.percentage >= 25 && status.warning.is_none() {
            *self = Self::default();
        } else if status.percentage >= 10 && status.warning != Some(BatteryWarning::Critical) {
            self.critical_sent = false;
        }
        match status.warning {
            Some(BatteryWarning::Low) if !self.low_sent => Some(BatteryWarning::Low),
            Some(BatteryWarning::Critical) if !self.critical_sent => Some(BatteryWarning::Critical),
            _ => None,
        }
    }

    pub fn delivered(&mut self, warning: BatteryWarning) {
        self.low_sent = true;
        if warning == BatteryWarning::Critical {
            self.critical_sent = true;
        }
    }
}
