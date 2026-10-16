use std::time::Duration;

use futures_util::{StreamExt, stream};
use tokio::sync::mpsc::UnboundedSender;
use zbus::{
    Connection, MatchRule, MessageStream,
    connection::Builder,
    fdo::{DBusProxy, PropertiesProxy},
    message::Type,
    names::InterfaceName,
};

use super::{BatteryStatus, model, notifications::Notifications, policy::Warnings};

pub(super) const SERVICE: &str = "org.freedesktop.UPower";
pub(super) const ROOT: &str = "/org/freedesktop/UPower";
pub(super) const DISPLAY: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
pub(super) const DEVICE: &str = "org.freedesktop.UPower.Device";

pub(super) struct Config {
    pub system_address: Option<String>,
    pub poll: Duration,
    pub retry: Duration,
    pub timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            system_address: None,
            poll: Duration::from_secs(30),
            retry: Duration::from_secs(2),
            timeout: Duration::from_secs(2),
        }
    }
}

/// Monitor UPower off the UI thread. Dropping the receiver stops the worker.
/// Only an explicit private session address enables notification delivery.
pub async fn monitor(
    updates: UnboundedSender<Option<BatteryStatus>>,
    notification_bus_address: Option<String>,
) {
    run(updates, notification_bus_address, Config::default()).await;
}

pub(super) async fn run(
    updates: UnboundedSender<Option<BatteryStatus>>,
    notification_bus_address: Option<String>,
    config: Config,
) {
    let mut previous = None;
    let mut warnings = Warnings::default();
    let mut notifications = Notifications::new(notification_bus_address);
    let _ = updates.send(None);
    loop {
        if updates.is_closed() {
            return;
        }
        if let Ok(Ok(session)) = tokio::time::timeout(config.timeout, connect(&config)).await {
            let _ = session
                .observe(
                    &updates,
                    &mut previous,
                    &mut warnings,
                    &mut notifications,
                    &config,
                )
                .await;
        }
        publish(&updates, &mut previous, None);
        tokio::select! {
            _ = updates.closed() => return,
            _ = tokio::time::sleep(config.retry) => {},
        }
    }
}

fn publish(
    updates: &UnboundedSender<Option<BatteryStatus>>,
    previous: &mut Option<BatteryStatus>,
    next: Option<BatteryStatus>,
) {
    if *previous != next {
        *previous = next;
        let _ = updates.send(next);
    }
}

struct Session {
    connection: Connection,
    properties: MessageStream,
    owners: MessageStream,
}

async fn connect(config: &Config) -> Result<Session, String> {
    let builder = match config.system_address.as_deref() {
        Some(address) => Builder::address(address),
        None => Builder::system(),
    }
    .map_err(error)?;
    let connection = builder
        .method_timeout(Duration::from_millis(750))
        .max_queued(32)
        .build()
        .await
        .map_err(error)?;
    // Subscribe before reading; invalidated and changed properties both trigger
    // a fresh GetAll snapshot rather than trusting a property cache.
    let properties = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(SERVICE)
        .map_err(error)?
        .path_namespace(ROOT)
        .map_err(error)?
        .build();
    let properties = MessageStream::for_match_rule(properties, &connection, Some(32))
        .await
        .map_err(error)?;
    let owners = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.freedesktop.DBus")
        .map_err(error)?
        .interface("org.freedesktop.DBus")
        .map_err(error)?
        .member("NameOwnerChanged")
        .map_err(error)?
        .add_arg(SERVICE)
        .map_err(error)?
        .build();
    let owners = MessageStream::for_match_rule(owners, &connection, Some(8))
        .await
        .map_err(error)?;
    Ok(Session {
        connection,
        properties,
        owners,
    })
}

impl Session {
    async fn observe(
        self,
        updates: &UnboundedSender<Option<BatteryStatus>>,
        previous: &mut Option<BatteryStatus>,
        warnings: &mut Warnings,
        notifications: &mut Notifications,
        config: &Config,
    ) -> Result<(), String> {
        let mut signals = stream::select(self.properties, self.owners);
        loop {
            let status = tokio::time::timeout(config.timeout, read(&self.connection))
                .await
                .map_err(error)??;
            publish(updates, previous, status);
            if let Some(warning) = warnings.pending(status)
                && notifications.send(status.unwrap(), warning).await
            {
                warnings.delivered(warning);
            }
            tokio::select! {
                _ = updates.closed() => return Ok(()),
                _ = tokio::time::sleep(config.poll) => {},
                message = signals.next() => {
                    message.ok_or("UPower signal connection closed")?.map_err(error)?;
                },
            }
        }
    }
}

async fn read(connection: &Connection) -> Result<Option<BatteryStatus>, String> {
    let bus = DBusProxy::new(connection).await.map_err(error)?;
    let name = SERVICE.try_into().expect("valid UPower service name");
    let owner = match bus.get_name_owner(name).await {
        Ok(owner) => owner,
        Err(zbus::fdo::Error::NameHasNoOwner(_)) => {
            // UPower may be installed but D-Bus activated on first use. Merely
            // querying ownership never activates it. This stays inside the
            // snapshot deadline and respects the system bus activation policy.
            let name = SERVICE.try_into().expect("valid UPower service name");
            bus.start_service_by_name(name, 0).await.map_err(error)?;
            let name = SERVICE.try_into().expect("valid UPower service name");
            bus.get_name_owner(name).await.map_err(error)?
        }
        Err(err) => return Err(error(err)),
    };
    // Pin all properties to one daemon generation and verify it still owns the
    // name afterwards. A stale response from a replaced daemon is never shown.
    let display = properties(connection, owner.as_str(), DISPLAY, DEVICE).await?;
    let power = properties(connection, owner.as_str(), ROOT, SERVICE).await?;
    let on_battery = power
        .get("OnBattery")
        .and_then(|value| bool::try_from(value).ok())
        .ok_or("UPower has no valid OnBattery property")?;
    let name = SERVICE.try_into().expect("valid UPower service name");
    if bus.get_name_owner(name).await.map_err(error)? != owner {
        return Err("UPower changed owner during a snapshot".into());
    }
    Ok(model::decode(&display, on_battery))
}

async fn properties(
    connection: &Connection,
    owner: &str,
    path: &str,
    interface: &str,
) -> Result<model::Properties, String> {
    PropertiesProxy::builder(connection)
        .destination(owner)
        .map_err(error)?
        .path(path)
        .map_err(error)?
        .build()
        .await
        .map_err(error)?
        .get_all(InterfaceName::try_from(interface).map_err(error)?)
        .await
        .map_err(error)
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
