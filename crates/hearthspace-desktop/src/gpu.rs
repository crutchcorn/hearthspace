//! Best-effort per-child GPU offload, independent of compositor rendering.
use std::{
    collections::HashMap,
    io,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use zbus::{Connection, Proxy, proxy::CacheProperties, zvariant::OwnedValue};

const SERVICE: &str = "net.hadess.SwitcherooControl";
const PATH: &str = "/net/hadess/SwitcherooControl";
const TIMEOUT: Duration = Duration::from_millis(500);

/// Resolve only on a launch worker. Absence, timeout or malformed service data
/// leave the ordinary launch intact: the desktop key is a preference, not a
/// promise to restart an existing instance or force an integrated GPU.
pub fn launch_environment(prefers_non_default_gpu: bool) -> Vec<(String, String)> {
    if !prefers_non_default_gpu {
        return Vec::new();
    }
    let result = crate::activation::operations::bounded(
        Instant::now() + TIMEOUT,
        &AtomicBool::new(false),
        async {
            let connection = Connection::system().await.map_err(failed)?;
            query(&connection).await
        },
    );
    result.unwrap_or_else(|error| {
        tracing::debug!(%error, "GPU preference unavailable; using ordinary application launch");
        Vec::new()
    })
}

async fn query(connection: &Connection) -> io::Result<Vec<(String, String)>> {
    let proxy = zbus::proxy::Builder::<Proxy<'_>>::new(connection)
        .destination(SERVICE)
        .map_err(failed)?
        .path(PATH)
        .map_err(failed)?
        .interface(SERVICE)
        .map_err(failed)?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .map_err(failed)?;
    let devices: Vec<HashMap<String, OwnedValue>> =
        proxy.get_property("GPUs").await.map_err(failed)?;
    select_environment(devices)
}

struct Gpu {
    default: bool,
    discrete: bool,
    environment: Option<OwnedValue>,
}

fn select_environment(
    devices: Vec<HashMap<String, OwnedValue>>,
) -> io::Result<Vec<(String, String)>> {
    if devices.len() > 64 {
        return Err(io::Error::other("GPU inventory is too large"));
    }
    let devices = devices
        .into_iter()
        .map(|mut properties| {
            let default = properties
                .get("Default")
                .ok_or_else(|| io::Error::other("GPU has no default designation"))?
                .try_into()
                .map_err(failed)?;
            // Older switcheroo-control versions do not report Discrete.
            let discrete = properties
                .get("Discrete")
                .map(bool::try_from)
                .transpose()
                .map_err(failed)?
                .unwrap_or(false);
            Ok(Gpu {
                default,
                discrete,
                environment: properties.remove("Environment"),
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    // KIO policy: a discrete default already satisfies the preference. Otherwise
    // prefer the first explicitly discrete GPU, then the first non-default GPU.
    if devices.iter().any(|gpu| gpu.default && gpu.discrete) {
        return Ok(Vec::new());
    }
    let chosen = devices
        .iter()
        .position(|gpu| gpu.discrete)
        .or_else(|| devices.iter().position(|gpu| !gpu.default));
    let Some(index) = chosen else {
        return Ok(Vec::new());
    };
    let environment = devices
        .into_iter()
        .nth(index)
        .unwrap()
        .environment
        .ok_or_else(|| io::Error::other("GPU has no launch environment"))?;
    let values = Vec::<String>::try_from(environment).map_err(failed)?;
    if values.len() > 64 || !values.len().is_multiple_of(2) {
        return Err(io::Error::other("Invalid GPU launch environment"));
    }
    let mut result = Vec::new();
    for pair in values.as_chunks::<2>().0 {
        let (key, value) = (&pair[0], &pair[1]);
        if key.is_empty()
            || key.len() > 256
            || key
                .bytes()
                .any(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))
            || key.as_bytes()[0].is_ascii_digit()
            || value.contains('\0')
            || value.len() > 4096
            || result.iter().any(|(previous, _)| previous == key)
        {
            return Err(io::Error::other(
                "Invalid GPU launch environment assignment",
            ));
        }
        result.push((key.clone(), value.clone()));
    }
    Ok(result)
}

fn failed(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!("GPU preference lookup failed: {error}"))
}

#[cfg(test)]
mod tests;
