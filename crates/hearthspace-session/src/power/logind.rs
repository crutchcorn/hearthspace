//! Bounded logind transport. A connection belongs to one daemon incarnation.
use std::{future::Future, io, os::fd::OwnedFd, time::Duration};

use futures_util::StreamExt;
use zbus::{Connection, Proxy, proxy::SignalStream, zvariant::OwnedObjectPath};

const SERVICE: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER_INTERFACE: &str = "org.freedesktop.login1.Manager";
const SESSION_INTERFACE: &str = "org.freedesktop.login1.Session";
const RPC_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub session_id: String,
    pub session_path: String,
    pub active: bool,
    pub local_seat: bool,
    pub lid_closed: bool,
    pub docked: bool,
    pub preparing_for_sleep: bool,
    pub delay_max: Duration,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Event {
    PrepareForSleep(bool),
    SessionLock,
}

pub(crate) struct Logind {
    manager: Proxy<'static>,
    session: Proxy<'static>,
    bus: Proxy<'static>,
    owner: String,
    sleep_events: SignalStream<'static>,
    lock_events: SignalStream<'static>,
    owner_events: SignalStream<'static>,
    session_id: String,
    local_seat: bool,
}

impl Logind {
    pub(crate) async fn connect() -> io::Result<Self> {
        bounded(async {
            let connection = zbus::connection::Builder::system()?
                .method_timeout(RPC_TIMEOUT)
                .build()
                .await?;
            Self::from_connection(connection).await
        })
        .await
    }

    pub(super) async fn from_connection(connection: Connection) -> zbus::Result<Self> {
        let bus = proxy(
            &connection,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )
        .await?;
        // Subscribe before looking up the owner; a restart in this interval
        // causes a conservative reconnect instead of retaining invalid inhibits.
        let owner_events = bus
            .receive_signal_with_args("NameOwnerChanged", &[(0, SERVICE)])
            .await?;
        let owner: String = bus.call("GetNameOwner", &(SERVICE,)).await?;
        let manager = proxy(&connection, &owner, MANAGER_PATH, MANAGER_INTERFACE).await?;
        let session_path: OwnedObjectPath = manager
            .call("GetSessionByPID", &(std::process::id(),))
            .await?;
        let session = proxy(
            &connection,
            &owner,
            session_path.as_str(),
            SESSION_INTERFACE,
        )
        .await?;
        let session_id = session.get_property("Id").await?;
        let remote: bool = session.get_property("Remote").await?;
        let (seat, _): (String, OwnedObjectPath) = session.get_property("Seat").await?;
        let vt: u32 = session.get_property("VTNr").await?;
        let sleep_events = manager.receive_signal("PrepareForSleep").await?;
        let lock_events = session.receive_signal("Lock").await?;
        Ok(Self {
            manager,
            session,
            bus,
            owner,
            sleep_events,
            lock_events,
            owner_events,
            session_id,
            local_seat: !remote && !seat.is_empty() && vt > 0,
        })
    }

    pub(crate) async fn snapshot(&self) -> io::Result<Snapshot> {
        bounded(async {
            // Unique destinations cannot silently follow a restarted logind.
            // Checking ownership also covers a stopped daemon whose old bus
            // connection has not closed yet.
            let owner: String = self.bus.call("GetNameOwner", &(SERVICE,)).await?;
            if owner != self.owner {
                return Err(zbus::Error::Failure("logind owner changed".into()));
            }
            Ok(Snapshot {
                session_id: self.session_id.clone(),
                session_path: self.session.path().as_str().to_owned(),
                active: self.session.get_property("Active").await?,
                local_seat: self.local_seat,
                lid_closed: self.manager.get_property("LidClosed").await?,
                docked: self.manager.get_property("Docked").await?,
                preparing_for_sleep: self.manager.get_property("PreparingForSleep").await?,
                delay_max: Duration::from_micros(
                    self.manager.get_property("InhibitDelayMaxUSec").await?,
                ),
            })
        })
        .await
    }

    pub(crate) async fn take_sleep_delay(&self) -> io::Result<OwnedFd> {
        self.inhibit("sleep", "Present the session lock before sleep", "delay")
            .await
    }

    pub(crate) async fn take_lid_control(&self) -> io::Result<OwnedFd> {
        self.inhibit(
            "handle-lid-switch",
            "Active Hearthspace session handles the lid",
            "block",
        )
        .await
    }

    async fn inhibit(&self, what: &str, why: &str, mode: &str) -> io::Result<OwnedFd> {
        let fd: zbus::zvariant::OwnedFd = bounded(
            self.manager
                .call("Inhibit", &(what, "Hearthspace", why, mode)),
        )
        .await?;
        Ok(fd.into())
    }

    /// A timed-out call may already have initiated sleep. Never blindly retry it.
    pub(crate) async fn suspend(&self) -> io::Result<()> {
        bounded(self.manager.call("Suspend", &(false,))).await
    }

    pub(crate) async fn can_suspend(&self) -> io::Result<String> {
        bounded(self.manager.call("CanSuspend", &())).await
    }

    pub(crate) async fn set_locked_hint(&self, locked: bool) -> io::Result<()> {
        bounded(self.session.call("SetLockedHint", &(locked,))).await
    }

    /// The caller must select this wait alongside its poll and shutdown timers.
    pub(crate) async fn next_event(&mut self) -> io::Result<Event> {
        tokio::select! {
            biased;
            _ = self.owner_events.next() => {
                Err(io::Error::other("logind owner changed or system bus disconnected"))
            }
            message = self.sleep_events.next() => {
                let message = message.ok_or_else(|| io::Error::other("logind sleep stream ended"))?;
                let (preparing,): (bool,) = message.body().deserialize().map_err(io::Error::other)?;
                Ok(Event::PrepareForSleep(preparing))
            }
            message = self.lock_events.next() => {
                message.ok_or_else(|| io::Error::other("logind session stream ended"))?;
                Ok(Event::SessionLock)
            }
        }
    }
}

async fn proxy(
    connection: &Connection,
    destination: &str,
    path: &str,
    interface: &'static str,
) -> zbus::Result<Proxy<'static>> {
    zbus::proxy::Builder::new(connection)
        .destination(destination.to_owned())?
        .path(path.to_owned())?
        .interface(interface)?
        // Docked and PreparingForSleep explicitly do not emit change signals.
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
}

async fn bounded<T>(future: impl Future<Output = zbus::Result<T>>) -> io::Result<T> {
    tokio::time::timeout(RPC_TIMEOUT, future)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "logind operation timed out"))?
        .map_err(io::Error::other)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) mod test_support;
