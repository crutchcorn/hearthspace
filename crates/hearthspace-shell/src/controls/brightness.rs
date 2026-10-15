//! Native panel backlight control through the session's existing logind rights.
//!
//! Sysfs is read-only here; logind checks the foreground session, user and seat
//! again when applying a change. No elevated helper or writable device rule is
//! needed, and displays without a kernel backlight remain unsupported.
use std::{path::PathBuf, time::Duration};

mod logind;
mod sysfs;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

const DEADLINE: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrightnessStatus {
    pub percentage: u8,
    pub device: String,
}

/// Re-discovers devices and logind on every operation, so device hotplug and
/// daemon restarts cannot leave a stale control target behind.
pub struct Brightness {
    native: bool,
    root: PathBuf,
    system_address: Option<String>,
    timeout: Duration,
}

impl Brightness {
    /// `native` must come from the compositor backend, not an ambient desktop
    /// environment variable: a nested shell must never adjust its host display.
    pub fn new(native: bool) -> Self {
        Self {
            native,
            root: PathBuf::from("/sys/class/backlight"),
            system_address: None,
            timeout: DEADLINE,
        }
    }

    /// Returns `None` when no usable kernel backlight exists. Errors describe
    /// unavailable session permission or unreadable device state.
    pub async fn read(&self) -> Result<Option<BrightnessStatus>, String> {
        self.require_native()?;
        self.bounded(async {
            let Some(device) = sysfs::discover(&self.root).await? else {
                return Ok(None);
            };
            logind::Session::connect(self.system_address.as_deref()).await?;
            Ok(Some(device.status()))
        })
        .await
    }

    /// Sets brightness to 1–100%, preserving a nonzero backlight even when a
    /// keyboard decrement reaches zero. The returned value is read back from
    /// the driver; no optimistic or synthetic brightness is reported.
    pub async fn set(&self, percentage: u8) -> Result<BrightnessStatus, String> {
        self.set_guarded(percentage, || async { Ok(()) }).await
    }

    /// Revalidate compositor ownership/lock state after device discovery, as
    /// close to the hardware mutation as possible. Failed guards never write.
    pub async fn set_guarded<F, Fut>(
        &self,
        percentage: u8,
        authorize: F,
    ) -> Result<BrightnessStatus, String>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        self.require_native()?;
        if percentage > 100 {
            return Err("Brightness must be between 1% and 100%.".into());
        }
        self.bounded(async {
            let device = sysfs::discover(&self.root)
                .await?
                .ok_or("No display brightness control is available.")?;
            let session = logind::Session::connect(self.system_address.as_deref()).await?;
            session
                .set_guarded(&device.name, device.level(percentage), authorize)
                .await?;
            // Read exactly the device that was changed, not a newly preferred
            // device which may have appeared during the D-Bus round trip.
            Ok(sysfs::read(&self.root, &device.name).await?.status())
        })
        .await
    }

    fn require_native(&self) -> Result<(), String> {
        if self.native {
            Ok(())
        } else {
            Err("Brightness is available in a native Hearthspace session.".into())
        }
    }

    async fn bounded<T>(
        &self,
        operation: impl std::future::Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        tokio::time::timeout(self.timeout, operation)
            .await
            .map_err(|_| "The display brightness operation could not be confirmed.".to_string())?
    }
}
