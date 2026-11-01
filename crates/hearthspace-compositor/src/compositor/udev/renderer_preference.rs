use std::{ffi::OsString, fmt::Display, io, path::Path};

use tracing::{info, warn};

use super::{device::UdevDeviceInfo, identity::device_identity};

const ENV_KEY: &str = "HEARTHSPACE_DRM_RENDER_DEVICE";

/// A startup hint, retained across inactive-seat deferral and consumed by the
/// first canonical renderer. Recovery never promotes a returning preferred GPU.
#[derive(Debug, Default)]
pub(super) struct StartupRendererPreference {
    pending: Option<PendingPreference>,
}

#[derive(Debug)]
struct PendingPreference {
    identity: String,
    device_id: Option<u64>,
    fallback_reason: Option<String>,
}

impl StartupRendererPreference {
    pub(super) fn from_environment(devices: &[UdevDeviceInfo]) -> io::Result<Self> {
        Self::from_value(std::env::var_os(ENV_KEY), devices, device_identity)
    }

    fn from_value(
        value: Option<OsString>,
        devices: &[UdevDeviceInfo],
        resolve: impl FnMut(&Path) -> io::Result<String>,
    ) -> io::Result<Self> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let identity = value
            .into_string()
            .map_err(|_| invalid_preference("must contain a UTF-8 physical DRM identity"))?;
        if identity.is_empty() {
            return Err(invalid_preference("must not be empty"));
        }
        if !identity.starts_with("/sys/devices/") {
            return Err(invalid_preference(
                "must be the exact physical DRM identity under /sys/devices/",
            ));
        }
        let index = matching_device(&identity, devices, resolve)?;
        info!(requested = %identity, device_id = devices[index].device_id,
            path = %devices[index].path.display(), "validated startup DRM renderer preference");
        Ok(Self {
            pending: Some(PendingPreference {
                identity,
                device_id: Some(devices[index].device_id),
                fallback_reason: None,
            }),
        })
    }

    /// Never interpret the requested identity as a path to open. Only move the
    /// matching enumerated device; all remaining devices keep their auto order.
    pub(super) fn opening_order(
        &mut self,
        devices: &[UdevDeviceInfo],
        session_active: bool,
        resolve: impl FnMut(&Path) -> io::Result<String>,
    ) -> Vec<UdevDeviceInfo> {
        if !session_active {
            return Vec::new();
        }
        let mut ordered = devices.to_vec();
        let Some(pending) = self.pending.as_mut() else {
            return ordered;
        };
        // Activation replaces the enumeration, so resolve again instead of
        // retaining a card number or mutating the backend's automatic ordering.
        match matching_device(&pending.identity, devices, resolve) {
            Ok(index) => {
                pending.device_id = Some(devices[index].device_id);
                pending.fallback_reason = None;
                ordered[..=index].rotate_right(1);
            }
            Err(error) => {
                pending.device_id = None;
                pending.fallback_reason = Some(error.to_string());
                warn!(requested = %pending.identity, %error,
                    "startup DRM renderer preference unavailable; using automatic fallback order");
            }
        }
        ordered
    }

    pub(super) fn open_failed(&mut self, device_id: u64, stage: &str, error: &dyn Display) {
        if let Some(pending) = self.pending.as_mut()
            && pending.device_id == Some(device_id)
        {
            pending.fallback_reason = Some(format!("{stage}: {error}"));
            warn!(requested = %pending.identity, device_id, stage, %error,
                "preferred startup DRM renderer failed; trying remaining seat GPUs");
        }
    }

    /// Called only after device initialization and notifier registration succeed.
    /// The generic node keeps this selection boundary testable without DRM.
    pub(super) fn scene_after_open<Node: Copy>(
        &mut self,
        current: Option<Node>,
        opened: Node,
        identity: &str,
    ) -> Node {
        if let Some(current) = current {
            return current;
        }
        if let Some(pending) = self.pending.take() {
            let reason = if let Some(reason) = pending.fallback_reason.as_deref() {
                reason
            } else if pending.identity == identity {
                "preferred device opened"
            } else {
                "preferred physical identity changed before device opening"
            };
            info!(requested = %pending.identity, actual = identity, reason,
                "selected startup DRM renderer; preference consumed");
        }
        opened
    }
}

fn invalid_preference(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, format!("{ENV_KEY} {reason}"))
}

fn matching_device(
    requested: &str,
    devices: &[UdevDeviceInfo],
    mut resolve: impl FnMut(&Path) -> io::Result<String>,
) -> io::Result<usize> {
    let mut matched = None;
    for (index, device) in devices.iter().enumerate() {
        let identity = resolve(&device.path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "{ENV_KEY}={requested:?}: cannot resolve physical DRM identity for {}: {error}",
                    device.path.display()
                ),
            )
        })?;
        if identity == requested && matched.replace(index).is_some() {
            return Err(invalid_preference(&format!(
                "{requested:?} is ambiguous among enumerated seat GPUs"
            )));
        }
    }
    matched.ok_or_else(|| {
        invalid_preference(&format!(
            "{requested:?} does not match any enumerated seat GPU physical identity"
        ))
    })
}

#[cfg(test)]
mod tests;
