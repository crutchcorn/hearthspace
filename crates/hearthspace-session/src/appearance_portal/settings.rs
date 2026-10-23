use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::fallback::Fallback;
use hearthspace_common::{Appearance, ColorMode};
use zbus::{Connection, object_server::SignalEmitter, zvariant::OwnedValue};

pub(crate) const NAME: &str = "org.freedesktop.impl.portal.desktop.hearthspace";
pub(super) const PATH: &str = "/org/freedesktop/portal/desktop";
pub(super) const INTERFACE: &str = "org.freedesktop.impl.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";
const GNOME_INTERFACE: &str = "org.gnome.desktop.interface";
pub(super) type Values = HashMap<String, HashMap<String, OwnedValue>>;

#[derive(Clone)]
pub(super) struct Settings {
    appearance: Arc<Mutex<Appearance>>,
    fallback: Option<Fallback>,
}

impl Settings {
    #[cfg(test)]
    pub(super) fn new(appearance: Appearance) -> Self {
        Self {
            appearance: Arc::new(Mutex::new(appearance)),
            fallback: None,
        }
    }

    pub(super) fn with_fallback(appearance: Appearance, fallback: Fallback) -> Self {
        Self {
            appearance: Arc::new(Mutex::new(appearance)),
            fallback: Some(fallback),
        }
    }

    pub(super) async fn publish(
        &self,
        connection: &Connection,
        appearance: Appearance,
    ) -> zbus::Result<()> {
        let previous = {
            let mut current = self.appearance.lock().unwrap();
            std::mem::replace(&mut *current, appearance)
        };
        let emitter = SignalEmitter::new(connection, PATH)?;
        for (namespace, values) in values(appearance) {
            let previous = values_for(previous, &namespace);
            for (key, value) in values {
                if previous.get(&key) != Some(&value) {
                    Self::setting_changed(&emitter, &namespace, &key, value).await?;
                }
            }
        }
        Ok(())
    }

    pub(super) async fn announce(&self, connection: &Connection) -> zbus::Result<()> {
        let snapshot = *self.appearance.lock().unwrap();
        let emitter = SignalEmitter::new(connection, PATH)?;
        for (namespace, values) in values(snapshot) {
            for (key, value) in values {
                Self::setting_changed(&emitter, &namespace, &key, value).await?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.portal.Error")]
enum Error {
    NotFound(String),
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Settings")]
impl Settings {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        1
    }

    async fn read_all(&self, namespaces: Vec<String>) -> Values {
        let mut merged = match &self.fallback {
            Some(fallback) => fallback.read_all(&namespaces).await,
            None => Values::new(),
        };
        let local = values(*self.appearance.lock().unwrap())
            .into_iter()
            .filter(|(namespace, _)| {
                namespaces.is_empty()
                    || namespaces
                        .iter()
                        .any(|pattern| namespace_matches(pattern, namespace))
            })
            .collect::<Values>();
        for (namespace, values) in local {
            merged.entry(namespace).or_default().extend(values);
        }
        merged
    }

    async fn read(&self, namespace: &str, key: &str) -> Result<OwnedValue, Error> {
        if let Some(value) = values_for(*self.appearance.lock().unwrap(), namespace).remove(key) {
            return Ok(value);
        }
        if let Some(fallback) = &self.fallback
            && let Some(value) = fallback.read(namespace, key).await
        {
            return Ok(value);
        }
        Err(Error::NotFound(format!(
            "Unknown setting {namespace}::{key}"
        )))
    }

    #[zbus(signal)]
    pub(super) async fn setting_changed(
        emitter: &SignalEmitter<'_>,
        namespace: &str,
        key: &str,
        value: OwnedValue,
    ) -> zbus::Result<()>;
}

pub(super) async fn connect(bus: &str, settings: Settings) -> zbus::Result<Connection> {
    let connection = zbus::connection::Builder::address(bus)?
        .serve_at(PATH, settings)?
        .build()
        .await?;
    // Do not replace or queue behind a foreign owner, even on our private bus.
    connection
        .request_name_with_flags(NAME, zbus::fdo::RequestNameFlags::DoNotQueue.into())
        .await?;
    Ok(connection)
}

fn values(appearance: Appearance) -> Values {
    [APPEARANCE, GNOME_INTERFACE]
        .into_iter()
        .map(|namespace| (namespace.into(), values_for(appearance, namespace)))
        .collect()
}

fn values_for(appearance: Appearance, namespace: &str) -> HashMap<String, OwnedValue> {
    let dark = appearance.color_mode == ColorMode::Dark;
    match namespace {
        APPEARANCE => {
            let [red, green, blue] = appearance.accent_color.srgb();
            HashMap::from([
                (
                    "color-scheme".into(),
                    appearance.color_mode.portal_value().into(),
                ),
                (
                    "accent-color".into(),
                    OwnedValue::try_from(zbus::zvariant::Value::from((red, green, blue)))
                        .expect("an RGB tuple is a valid D-Bus value"),
                ),
            ])
        }
        // GTK 3's Wayland backend only requests org.gnome.* from the portal.
        // Publishing these aliases gives it live light/dark changes without
        // writing the user's GSettings/dconf values.
        GNOME_INTERFACE => HashMap::from([
            (
                "gtk-theme".into(),
                zbus::zvariant::Str::from(if dark { "Adwaita-dark" } else { "Adwaita" }).into(),
            ),
            (
                "color-scheme".into(),
                zbus::zvariant::Str::from(if dark { "prefer-dark" } else { "prefer-light" }).into(),
            ),
        ]),
        _ => HashMap::new(),
    }
}

pub(super) fn namespace_matches(pattern: &str, namespace: &str) -> bool {
    // The portal specification permits a wildcard at the end, not arbitrary
    // glob expressions. An empty list and "*" both request all namespaces.
    pattern
        .strip_suffix('*')
        .map_or(pattern == namespace, |prefix| namespace.starts_with(prefix))
}

pub(super) fn owns(namespace: &str, key: &str) -> bool {
    matches!(
        (namespace, key),
        (APPEARANCE, "color-scheme" | "accent-color")
            | (GNOME_INTERFACE, "gtk-theme" | "color-scheme")
    )
}
