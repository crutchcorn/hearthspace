//! User-requested sleep shares the lid path's presented-lock requirement.
use super::*;

#[derive(Default)]
pub(super) struct Manual {
    pending: Option<(Instant, Request)>,
    dispatched: bool,
}

impl Manual {
    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.dispatched
    }

    pub fn sleep_event(&mut self) {
        // A concurrent external sleep request also satisfies a pending request.
        if let Some((_, request)) = self.pending.take() {
            request.finish(Ok(()));
        }
        self.dispatched = false;
    }
}

impl Cycle {
    fn unavailable(&self, snapshot: &Snapshot, status: &Status) -> Option<&'static str> {
        if self.manual.busy()
            || self.sleeping
            || self.preparing.is_some()
            || self.lid_request.is_some()
        {
            Some("Suspend is already in progress")
        } else if !snapshot.local_seat || !snapshot.active {
            Some("Suspend requires the active local desktop session")
        } else if snapshot.preparing_for_sleep || self.recovering.is_some() {
            Some("The desktop is recovering from sleep")
        } else if self.delay.is_none()
            || !self.resume_accepted
            || !status.recovery_ready
            || status.phase != "awake"
        {
            Some("The desktop is not ready to suspend safely")
        } else {
            None
        }
    }

    pub async fn manual_capability(
        &self,
        logind: &Logind,
        snapshot: &Snapshot,
        status: &Status,
    ) -> Reply {
        if let Some(reason) = self.unavailable(snapshot, status) {
            return Reply {
                busy: self.manual.busy() || self.sleeping || self.preparing.is_some(),
                ..Reply::unavailable(reason)
            };
        }
        match permission(logind).await {
            Ok(()) => Reply {
                ok: true,
                available: true,
                busy: false,
                error: None,
            },
            Err(error) => Reply::unavailable(error),
        }
    }

    pub async fn request_suspend(
        &mut self,
        request: Request,
        logind: &Logind,
        socket: &Path,
        status: &mut Status,
    ) {
        let result = async {
            let snapshot = logind.snapshot().await.map_err(|e| e.to_string())?;
            if let Some(reason) = self.unavailable(&snapshot, status) {
                return Err(reason.to_owned());
            }
            permission(logind).await?;
            let compositor = control::command(socket, "status")
                .await
                .map_err(|e| e.to_string())?;
            if compositor["active"] != true
                || compositor["recovery_ready"] != true
                || (compositor["locked"] != false && !control::confirmed(&compositor))
            {
                return Err("The active desktop is not ready to suspend safely".into());
            }
            if request.cancelled() {
                return Err("Suspend request expired".into());
            }
            control::command(socket, "lock-shield")
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        .await;
        match result {
            Ok(()) => {
                self.manual.pending = Some((Instant::now() + Duration::from_secs(8), request))
            }
            Err(error) => request.finish(Err(error)),
        }
    }

    pub async fn tick_manual(
        &mut self,
        snapshot: &Snapshot,
        logind: &Logind,
        socket: &Path,
        status: &mut Status,
        compositor: &serde_json::Value,
    ) {
        // After a rejected or ambiguous call, require an actual unlock before a
        // new explicit attempt. There is never an automatic retry.
        if self.manual.dispatched && compositor["active"] == true && compositor["locked"] == false {
            self.manual.dispatched = false;
        }
        let Some((deadline, request)) = self.manual.pending.take() else {
            return;
        };
        let error = if request.cancelled() || Instant::now() >= deadline {
            Some("Suspend cancelled: the lock was not presented in time")
        } else if !snapshot.active
            || !snapshot.local_seat
            || self.delay.is_none()
            || !status.recovery_ready
        {
            Some("Suspend cancelled: the active desktop is no longer ready")
        } else {
            None
        };
        if let Some(error) = error {
            request.finish(Err(error.into()));
            return;
        }
        if !status.lock_confirmed {
            self.manual.pending = Some((deadline, request));
            return;
        }
        let result = async {
            let current = logind.snapshot().await.map_err(|e| e.to_string())?;
            if !current.active || !current.local_seat || current.preparing_for_sleep {
                return Err("Suspend cancelled: the active session changed".into());
            }
            permission(logind).await?;
            // Capability queries may have yielded: recheck the presented shield
            // and native seat immediately before handing off to logind.
            let presented = control::command(socket, "status")
                .await
                .map_err(|e| e.to_string())?;
            if !control::confirmed(&presented)
                || presented["active"] != true
                || presented["recovery_ready"] != true
                || request.cancelled()
            {
                return Err("Suspend cancelled: the protected desktop changed".into());
            }
            let current = logind.snapshot().await.map_err(|e| e.to_string())?;
            if !current.active
                || !current.local_seat
                || current.preparing_for_sleep
                || request.cancelled()
            {
                return Err("Suspend cancelled: the active session changed".into());
            }
            self.manual.dispatched = true;
            logind
                .suspend()
                .await
                .map_err(|e| format!("Suspend failed: {e}"))
        }
        .await;
        if let Err(error) = &result {
            status.error(error);
        }
        request.finish(result);
    }
}

async fn permission(logind: &Logind) -> Result<(), String> {
    match logind.can_suspend().await.as_deref() {
        Ok("yes") => Ok(()),
        Ok("challenge") => Err("Suspend requires administrator authorization".into()),
        Ok("no") => Err("Suspend is not permitted for this session".into()),
        Ok(_) => Err("Suspend is not supported on this system".into()),
        Err(error) => Err(format!("Suspend capability is unavailable: {error}")),
    }
}
