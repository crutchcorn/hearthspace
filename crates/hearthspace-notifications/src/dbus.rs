use std::collections::HashMap;

use zbus::{fdo, message::Header, zvariant::OwnedValue};

use crate::model::{Incoming, Shared};

pub(crate) const NAME: &str = "org.freedesktop.Notifications";
pub(crate) const PATH: &str = "/org/freedesktop/Notifications";

pub(crate) struct Notifications(pub Shared);

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    fn get_capabilities(&self) -> Vec<&str> {
        vec!["actions", "body"]
    }

    #[zbus(out_args("name", "vendor", "version", "spec_version"))]
    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        (
            "Hearthspace",
            "Hearthspace",
            env!("CARGO_PKG_VERSION"),
            "1.3",
        )
    }

    #[allow(clippy::too_many_arguments)] // The signature is fixed by the desktop notification specification.
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<u32> {
        let _ = app_icon; // We do not advertise or render application-supplied icons.
        let sender = header
            .sender()
            .ok_or_else(|| fdo::Error::Failed("Missing sender".into()))?;
        let urgency = hints
            .get("urgency")
            .and_then(|value| u8::try_from(value).ok())
            .unwrap_or(1);
        let flag = |name| {
            hints
                .get(name)
                .and_then(|value| bool::try_from(value).ok())
                .unwrap_or(false)
        };
        self.0
            .lock()
            .unwrap()
            .notify(
                Incoming {
                    sender: sender.as_str(),
                    replaces_id,
                    app_name,
                    summary,
                    body,
                    actions: &actions,
                    urgency,
                    resident: flag("resident"),
                    transient: flag("transient"),
                    expire_timeout,
                },
                std::time::Instant::now(),
            )
            .map_err(fdo::Error::InvalidArgs)
    }

    fn close_notification(&self, id: u32, #[zbus(header)] header: Header<'_>) -> fdo::Result<()> {
        let sender = header
            .sender()
            .ok_or_else(|| fdo::Error::Failed("Missing sender".into()))?;
        self.0
            .lock()
            .unwrap()
            .close(id, 3, Some(sender.as_str()))
            .map_err(fdo::Error::Failed)
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}
