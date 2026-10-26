//! Confirmed reboot/power-off requests reuse the private coordinator and logind.
use super::*;

impl Cycle {
    fn shutdown_unavailable(&self, snapshot: &Snapshot) -> Option<&'static str> {
        if self.shared.shutdown_dispatched()
            || self.manual.busy()
            || self.lid_request.is_some()
            || self.sleeping
            || self.preparing.is_some()
        {
            Some("A power action is already in progress")
        } else if !snapshot.local_seat || !snapshot.active {
            Some("Power actions require the active local desktop session")
        } else if snapshot.preparing_for_sleep || self.recovering.is_some() {
            Some("The desktop is recovering from sleep")
        } else {
            None
        }
    }

    pub async fn capabilities(
        &self,
        logind: &Logind,
        snapshot: &Snapshot,
        status: &Status,
    ) -> Reply {
        let mut reply = self.manual_capability(logind, snapshot, status).await;
        let unavailable = if let Some(reason) = self.shutdown_unavailable(snapshot) {
            Some(reason.to_owned())
        } else {
            match logind.preparing_for_shutdown().await {
                Ok(true) => {
                    return Reply {
                        busy: true,
                        ..Reply::unavailable("Shutdown is already in progress")
                    };
                }
                Ok(false) => logind
                    .shutdown_blocker()
                    .await
                    .unwrap_or_else(|error| Some(error.to_string())),
                Err(error) => Some(error.to_string()),
            }
        };
        if let Some(reason) = unavailable {
            reply.reboot_error = Some(reason.clone());
            reply.power_off_error = Some(reason);
        } else {
            let (reboot, power_off) = tokio::join!(
                permission(logind, Action::Reboot),
                permission(logind, Action::PowerOff),
            );
            reply.reboot_available = reboot.is_ok();
            reply.reboot_error = reboot.err();
            reply.power_off_available = power_off.is_ok();
            reply.power_off_error = power_off.err();
        }
        if self.shared.shutdown_dispatched() {
            reply.busy = true;
            reply.available = false;
            reply.error = Some("A power action is already in progress".into());
        }
        reply
    }

    pub async fn request_power(
        &mut self,
        request: Request,
        logind: &Logind,
        socket: &Path,
        status: &mut Status,
    ) {
        if request.action == Action::Suspend && !self.shared.shutdown_dispatched() {
            self.request_suspend(request, logind, socket, status).await;
            return;
        }
        let action = request.action;
        let result = tokio::time::timeout(Duration::from_secs(12), async {
            let snapshot = logind.snapshot().await.map_err(|e| e.to_string())?;
            if let Some(reason) = self.shutdown_unavailable(&snapshot) {
                return Err(reason.to_owned());
            }
            permission(logind, action).await?;
            shutdown_ready(logind).await?;
            let compositor = control::command(socket, "status")
                .await
                .map_err(|e| e.to_string())?;
            if compositor["active"] != true || compositor["recovery_ready"] != true {
                return Err("Power action cancelled: the active desktop is not ready".into());
            }
            // Capabilities and compositor checks yield: revalidate our own seat
            // immediately before dispatch, never an unrelated active session.
            let snapshot = logind.snapshot().await.map_err(|e| e.to_string())?;
            if let Some(reason) = self.shutdown_unavailable(&snapshot) {
                return Err(reason.to_owned());
            }
            if request.cancelled() {
                return Err("Power request expired".into());
            }
            self.shared.set_shutdown_dispatched(true);
            match logind.shutdown(action).await {
                Ok(()) => Ok(()),
                Err(error) => {
                    // A concrete D-Bus rejection permits a new explicit attempt.
                    // A timeout/disconnect might have already scheduled shutdown;
                    // retain the gate instead of issuing a second action.
                    if rejected(&error) {
                        self.shared.set_shutdown_dispatched(false);
                    }
                    Err(format!(
                        "{} failed: {error}; it will not be retried automatically",
                        action.label()
                    ))
                }
            }
        })
        .await
        .unwrap_or_else(|_| {
            Err(format!(
                "{} timed out; it will not be retried automatically",
                action.label()
            ))
        });
        request.finish(result);
    }
}

async fn permission(logind: &Logind, action: Action) -> Result<(), String> {
    let label = action.label();
    match logind.can_shutdown(action).await.as_deref() {
        Ok("yes") => Ok(()),
        Ok("challenge") => Err(format!("{label} requires administrator authorization")),
        Ok("no") => Err(format!("{label} is not permitted for this session")),
        Ok(_) => Err(format!("{label} is not supported on this system")),
        Err(error) => Err(format!("{label} capability is unavailable: {error}")),
    }
}

async fn shutdown_ready(logind: &Logind) -> Result<(), String> {
    if logind
        .preparing_for_shutdown()
        .await
        .map_err(|e| e.to_string())?
    {
        return Err("Shutdown is already in progress".into());
    }
    if let Some(reason) = logind.shutdown_blocker().await.map_err(|e| e.to_string())? {
        return Err(reason);
    }
    Ok(())
}

fn rejected(error: &io::Error) -> bool {
    matches!(error.get_ref().and_then(|e| e.downcast_ref::<zbus::Error>()),
        Some(zbus::Error::MethodError(name, _, _))
            if !matches!(name.as_str(), "org.freedesktop.DBus.Error.NoReply"
                | "org.freedesktop.DBus.Error.Timeout" | "org.freedesktop.DBus.Error.Disconnected"))
}
