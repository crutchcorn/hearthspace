use std::{collections::BTreeMap, io};

use zbus::{Connection, Proxy, proxy::CacheProperties};

use super::{KEYS, operations::TIMEOUT};

pub struct Manager {
    proxy: Proxy<'static>,
    _connection: Connection,
}

impl Manager {
    pub async fn connect(bus: &str) -> io::Result<Self> {
        let connection = zbus::connection::Builder::address(bus)
            .map_err(failed)?
            .method_timeout(TIMEOUT)
            .build()
            .await
            .map_err(failed)?;
        let proxy = zbus::proxy::Builder::<Proxy<'_>>::new(&connection)
            .destination("org.freedesktop.systemd1")
            .map_err(failed)?
            .path("/org/freedesktop/systemd1")
            .map_err(failed)?
            .interface("org.freedesktop.systemd1.Manager")
            .map_err(failed)?
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .map_err(failed)?;
        Ok(Self {
            proxy,
            _connection: connection,
        })
    }

    pub async fn environment(&self) -> io::Result<BTreeMap<String, String>> {
        // systemd has an Environment property, not a per-key getter. Filter
        // immediately; never persist, log or return any unrelated variable.
        let all: Vec<String> = self
            .proxy
            .get_property("Environment")
            .await
            .map_err(failed)?;
        Ok(all
            .into_iter()
            .filter_map(|entry| {
                let (key, value) = entry.split_once('=')?;
                KEYS.contains(&key)
                    .then(|| (key.to_owned(), value.to_owned()))
            })
            .collect())
    }

    pub async fn replace(
        &self,
        unset: &[String],
        set: &BTreeMap<String, String>,
    ) -> io::Result<()> {
        let set: Vec<_> = set
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect();
        self.proxy
            .call::<_, _, ()>("UnsetAndSetEnvironment", &(unset, set))
            .await
            .map_err(failed)
    }
}

fn failed(_: zbus::Error) -> io::Error {
    io::Error::other("activation user-manager operation failed")
}
