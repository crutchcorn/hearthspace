//! Application-scoped pairing. Only BlueZ's current owner may use the agent.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use zbus::message::Header;
struct Cancellation {
    cancelled: AtomicBool,
    notify: tokio::sync::Notify,
}
impl Cancellation {
    async fn wait(&self) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.cancelled.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }
}
struct Guard(Arc<Cancellation>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.cancelled.store(true, Ordering::SeqCst);
        self.0.notify.notify_waiters();
    }
}
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum PairError {
    Rejected(String),
    Canceled(String),
}
struct Agent {
    owner: String,
    device: OwnedObjectPath,
    cancel: Arc<Cancellation>,
    display: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}
impl Agent {
    fn check(
        &self,
        device: &OwnedObjectPath,
        header: &Header<'_>,
    ) -> std::result::Result<(), PairError> {
        if device != &self.device
            || header.sender().map(|s| s.as_str()) != Some(&self.owner)
            || self.cancel.cancelled.load(Ordering::SeqCst)
        {
            return Err(PairError::Rejected(
                "Pairing request is no longer authorized.".into(),
            ));
        }
        Ok(())
    }
    async fn wait<T>(
        &self,
        f: impl std::future::Future<Output = Result<T>>,
    ) -> std::result::Result<T, PairError> {
        tokio::select! {
            biased;
            _=self.cancel.wait()=>Err(PairError::Canceled("Pairing cancelled.".into())),
            r=f=>r.map_err(|_|PairError::Canceled("Pairing cancelled.".into())),
        }
    }
}
pub(super) async fn pair(c: &Connection, id: &str) -> Result<()> {
    let device = target(c, "org.bluez", id, "/org/bluez/").await?;
    let destination = id.split_once('|').expect("validated device identity").0;
    let cancel = Arc::new(Cancellation {
        cancelled: AtomicBool::new(false),
        notify: tokio::sync::Notify::new(),
    });
    let _guard = Guard(cancel.clone());
    let agent_path = "/org/hearthspace/WirelessAgent";
    c.object_server()
        .at(
            agent_path,
            Agent {
                owner: destination.into(),
                device,
                cancel: cancel.clone(),
                display: Default::default(),
            },
        )
        .await
        .map_err(failure)?;
    let manager = proxy(c, destination, "/org/bluez", "org.bluez.AgentManager1").await?;
    let object = OwnedObjectPath::try_from(agent_path).map_err(failure)?;
    let registration: std::result::Result<(), _> = manager
        .call("RegisterAgent", &(&object, "KeyboardDisplay"))
        .await;
    if let Err(error) = registration {
        let _ = c.object_server().remove::<Agent, _>(agent_path).await;
        return Err(failure(error));
    }
    let result = super::bluetooth::device_call(c, id, "Pair").await;
    let cancelled = cancel.cancelled.load(Ordering::SeqCst);
    let _: std::result::Result<(), _> = manager.call("UnregisterAgent", &(&object,)).await;
    let _ = c.object_server().remove::<Agent, _>(agent_path).await;
    result?;
    if cancelled {
        return Err("Pairing cancelled.".into());
    }
    super::bluetooth::device_call(c, id, "Connect").await
}
#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    async fn release(&self, #[zbus(header)] h: Header<'_>) -> std::result::Result<(), PairError> {
        self.check(&self.device, &h)?;
        self.cancel.cancelled.store(true, Ordering::SeqCst);
        self.cancel.notify.notify_waiters();
        Ok(())
    }
    async fn cancel(&self, #[zbus(header)] h: Header<'_>) -> std::result::Result<(), PairError> {
        self.release(h).await
    }
    async fn request_pin_code(
        &self,
        device: OwnedObjectPath,
        #[zbus(header)] h: Header<'_>,
    ) -> std::result::Result<String, PairError> {
        self.check(&device, &h)?;
        self.wait(super::prompt::pin()).await
    }
    async fn request_passkey(
        &self,
        device: OwnedObjectPath,
        #[zbus(header)] h: Header<'_>,
    ) -> std::result::Result<u32, PairError> {
        self.check(&device, &h)?;
        self.wait(super::prompt::pin())
            .await?
            .parse::<u32>()
            .ok()
            .filter(|v| *v <= 999999)
            .ok_or_else(|| PairError::Rejected("Enter a six-digit code.".into()))
    }
    async fn request_confirmation(
        &self,
        device: OwnedObjectPath,
        passkey: u32,
        #[zbus(header)] h: Header<'_>,
    ) -> std::result::Result<(), PairError> {
        self.check(&device, &h)?;
        self.wait(super::prompt::confirm(&format!(
            "Does {passkey:06} match the code on your Bluetooth device?"
        )))
        .await
    }
    async fn request_authorization(
        &self,
        device: OwnedObjectPath,
        #[zbus(header)] h: Header<'_>,
    ) -> std::result::Result<(), PairError> {
        self.check(&device, &h)?;
        self.wait(super::prompt::confirm(
            "Allow this Bluetooth device to pair?",
        ))
        .await
    }
    async fn authorize_service(
        &self,
        device: OwnedObjectPath,
        _uuid: String,
        #[zbus(header)] h: Header<'_>,
    ) -> std::result::Result<(), PairError> {
        self.request_authorization(device, h).await
    }
    async fn display_pin_code(
        &self,
        device: OwnedObjectPath,
        pincode: String,
        #[zbus(header)] h: Header<'_>,
    ) -> std::result::Result<(), PairError> {
        self.check(&device, &h)?;
        let cancel = self.cancel.clone();
        let mut display = self.display.lock().unwrap();
        if let Some(task) = display.take() {
            task.abort();
        }
        *display = Some(tokio::spawn(async move {
            let text = format!("Enter {pincode} on your Bluetooth device.");
            tokio::select! {
                _=cancel.wait()=>{},
                _=super::prompt::info(&text)=>{},
            }
        }));
        Ok(())
    }
    async fn display_passkey(
        &self,
        device: OwnedObjectPath,
        passkey: u32,
        _entered: u16,
        #[zbus(header)] h: Header<'_>,
    ) -> std::result::Result<(), PairError> {
        self.display_pin_code(device, format!("{passkey:06}"), h)
            .await
    }
}
impl Drop for Agent {
    fn drop(&mut self) {
        if let Some(task) = self.display.lock().unwrap().take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_is_latched_for_late_authentication_and_wakes_existing_requests() {
        let cancel = Arc::new(Cancellation {
            cancelled: AtomicBool::new(false),
            notify: tokio::sync::Notify::new(),
        });
        let guard = Guard(cancel.clone());
        let waiting = cancel.clone();
        let pending = tokio::spawn(async move {
            waiting.wait().await;
        });
        tokio::task::yield_now().await;
        drop(guard);
        tokio::time::timeout(Duration::from_millis(200), pending)
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_millis(200), cancel.wait())
            .await
            .unwrap();
    }
}
