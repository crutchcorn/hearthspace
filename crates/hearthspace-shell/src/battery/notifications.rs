use std::{collections::HashMap, time::Duration};
use tokio::time::Instant;
use zbus::{Connection, Proxy, connection::Builder, zvariant::Value};

use super::{BatteryStatus, BatteryWarning};

pub(super) struct Notifications {
    address: Option<String>,
    connection: Option<Connection>,
    retry_at: Option<Instant>,
}

impl Notifications {
    pub fn new(address: Option<String>) -> Self {
        Self {
            address: address.filter(|value| !value.is_empty()),
            connection: None,
            retry_at: None,
        }
    }

    pub async fn send(&mut self, status: BatteryStatus, warning: BatteryWarning) -> bool {
        let Some(address) = self.address.as_deref() else {
            return false;
        };
        if self.retry_at.is_some_and(|next| Instant::now() < next) {
            return false;
        }
        let result = tokio::time::timeout(Duration::from_secs(2), async {
            if self.connection.is_none() {
                self.connection = Some(
                    Builder::address(address)?
                        .method_timeout(Duration::from_millis(750))
                        .build()
                        .await?,
                );
            }
            notify(self.connection.as_ref().unwrap(), status, warning).await
        })
        .await;
        if matches!(result, Ok(Ok(()))) {
            self.retry_at = None;
            true
        } else {
            // A missing/restarting notification service must not hide battery
            // updates, busy-loop, or silently mark an undelivered warning sent.
            self.connection = None;
            self.retry_at = Some(Instant::now() + Duration::from_secs(30));
            false
        }
    }
}

async fn notify(
    connection: &Connection,
    status: BatteryStatus,
    warning: BatteryWarning,
) -> zbus::Result<()> {
    let proxy = Proxy::new(
        connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .await?;
    let (title, icon, urgency) = match warning {
        BatteryWarning::Low => ("Low", "battery-low-symbolic", 1_u8),
        BatteryWarning::Critical => ("Critical", "battery-caution-symbolic", 2_u8),
    };
    let title = format!("{title} {}", status.kind.label().to_lowercase());
    let body = format!(
        "{}% remaining. Connect to power{}.",
        status.percentage,
        if warning == BatteryWarning::Critical {
            " and save your work"
        } else {
            ""
        }
    );
    let hints = HashMap::from([
        ("urgency", Value::U8(urgency)),
        ("category", Value::from("device")),
        ("desktop-entry", Value::from("hearthspace")),
    ]);
    let _: u32 = proxy
        .call(
            "Notify",
            &(
                "Hearthspace",
                0_u32,
                icon,
                title,
                body,
                Vec::<String>::new(),
                hints,
                -1_i32,
            ),
        )
        .await?;
    Ok(())
}
