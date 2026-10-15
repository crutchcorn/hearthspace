use std::time::Duration;

use zbus::{Connection, Proxy, proxy::CacheProperties, zvariant::OwnedObjectPath};

const SERVICE: &str = "org.freedesktop.login1";
const ROOT: &str = "/org/freedesktop/login1";
const INTERFACE: &str = "org.freedesktop.login1.Session";

pub(super) struct Session {
    bus: Proxy<'static>,
    session: Proxy<'static>,
    owner: String,
}

impl Session {
    pub async fn connect(address: Option<&str>) -> Result<Self, String> {
        let builder = match address {
            Some(address) => zbus::connection::Builder::address(address),
            None => zbus::connection::Builder::system(),
        }
        .map_err(error)?;
        let connection = builder
            .method_timeout(Duration::from_millis(750))
            .max_queued(16)
            .build()
            .await
            .map_err(error)?;
        let bus = proxy(
            &connection,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )
        .await?;
        let owner: String = bus.call("GetNameOwner", &(SERVICE,)).await.map_err(error)?;
        let manager = proxy(&connection, &owner, ROOT, "org.freedesktop.login1.Manager").await?;
        // Resolve this process rather than trusting XDG_SESSION_ID, and never
        // fall back to the user's other foreground session.
        let path: OwnedObjectPath = manager
            .call("GetSessionByPID", &(std::process::id(),))
            .await
            .map_err(error)?;
        let session = proxy(&connection, &owner, path.as_str(), INTERFACE).await?;
        let this = Self {
            bus,
            session,
            owner,
        };
        this.check().await?;
        Ok(this)
    }

    async fn check(&self) -> Result<(), String> {
        let owner: String = self
            .bus
            .call("GetNameOwner", &(SERVICE,))
            .await
            .map_err(error)?;
        if owner != self.owner {
            return Err("Display brightness service restarted. Try again.".into());
        }
        let remote: bool = self.session.get_property("Remote").await.map_err(error)?;
        let active: bool = self.session.get_property("Active").await.map_err(error)?;
        let (seat, _): (String, OwnedObjectPath) =
            self.session.get_property("Seat").await.map_err(error)?;
        let vt: u32 = self.session.get_property("VTNr").await.map_err(error)?;
        if remote || !active || seat.is_empty() || vt == 0 {
            return Err("Brightness requires the active local Hearthspace session.".into());
        }
        Ok(())
    }

    #[cfg(test)]
    pub async fn set(&self, device: &str, level: u32) -> Result<(), String> {
        self.set_guarded(device, level, || async { Ok(()) }).await
    }

    pub async fn set_guarded<F, Fut>(
        &self,
        device: &str,
        level: u32,
        mut authorize: F,
    ) -> Result<(), String>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        self.check().await?;
        authorize().await?;
        self.session
            .call::<_, _, ()>("SetBrightness", &("backlight", device, level))
            .await
            .map_err(error)
    }
}

async fn proxy(
    connection: &Connection,
    destination: &str,
    path: &str,
    interface: &'static str,
) -> Result<Proxy<'static>, String> {
    zbus::proxy::Builder::new(connection)
        .destination(destination.to_owned())
        .map_err(error)?
        .path(path.to_owned())
        .map_err(error)?
        .interface(interface)
        .map_err(error)?
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .map_err(error)
}

fn error(error: zbus::Error) -> String {
    tracing::debug!(%error, "display brightness service request failed");
    let name = match &error {
        zbus::Error::MethodError(name, _, _) => name.as_str(),
        _ => "",
    };
    match name {
        "org.freedesktop.DBus.Error.AccessDenied" | "org.freedesktop.login1.NotYourDevice" => {
            "Brightness permission was denied for this display."
        }
        "org.freedesktop.DBus.Error.UnknownMethod" | "org.freedesktop.DBus.Error.NotSupported" => {
            "This system does not support display brightness control."
        }
        "org.freedesktop.DBus.Error.ServiceUnknown"
        | "org.freedesktop.DBus.Error.NameHasNoOwner" => {
            "The system brightness service is unavailable."
        }
        _ => "Display brightness could not be confirmed. Try again.",
    }
    .into()
}
