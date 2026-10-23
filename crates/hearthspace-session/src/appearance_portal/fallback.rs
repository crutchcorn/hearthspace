//! Preserve unrelated GTK settings without allowing its host appearance values
//! to overwrite this session. Portal 1.21.1 forwards lower-priority signals even
//! when a higher-priority backend owns the corresponding key.
use std::time::Duration;

use futures_util::StreamExt;
use zbus::{Connection, Proxy, object_server::SignalEmitter, zvariant::OwnedValue};

use super::settings::{self, Settings, Values};

const NAME: &str = "org.freedesktop.impl.portal.desktop.gtk";
const TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone)]
pub(super) struct Fallback {
    proxy: Proxy<'static>,
}

impl Fallback {
    pub(super) async fn connect(bus: &str) -> zbus::Result<Self> {
        // Interface methods run on zbus's async-io executor, outside the
        // supervisor worker's Tokio runtime. Use zbus's own call timeout.
        let connection = zbus::connection::Builder::address(bus)?
            .method_timeout(TIMEOUT)
            .build()
            .await?;
        let proxy = zbus::proxy::Builder::<Proxy<'_>>::new(&connection)
            .destination(NAME)?
            .path(settings::PATH)?
            .interface(settings::INTERFACE)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await?;
        Ok(Self { proxy })
    }

    pub(super) async fn read_all(&self, namespaces: &[String]) -> Values {
        self.proxy
            .call("ReadAll", &(namespaces,))
            .await
            .unwrap_or_default()
    }

    pub(super) async fn read(&self, namespace: &str, key: &str) -> Option<OwnedValue> {
        self.proxy.call("Read", &(namespace, key)).await.ok()
    }

    pub(super) async fn forward(&self, destination: &Connection) -> zbus::Result<()> {
        // zbus tracks the well-known owner's changes, including GTK starting
        // after us and restarting under the existing process supervisor.
        let mut signals = self.proxy.receive_signal("SettingChanged").await?;
        let emitter = SignalEmitter::new(destination, settings::PATH)?;
        while let Some(message) = signals.next().await {
            let (namespace, key, value): (String, String, OwnedValue) =
                message.body().deserialize()?;
            if !settings::owns(&namespace, &key) {
                Settings::setting_changed(&emitter, &namespace, &key, value).await?;
            }
        }
        Err(zbus::Error::Failure(
            "GTK settings signal stream ended".into(),
        ))
    }
}
