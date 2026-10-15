//! UPower's system supply aggregate, with notifications on our private session bus.
//!
//! A missing daemon, absent supply, or invalid reading hides the indicator. No
//! peripheral enumeration, machine policy changes, or automatic power actions.

mod model;
mod notifications;
mod policy;
mod transport;

pub use model::{BatteryKind, BatteryState, BatteryStatus, BatteryWarning};
pub use transport::monitor;

#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
