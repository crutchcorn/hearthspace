use super::*;

#[cfg(test)]
mod tests;

impl Supervisor {
    pub(crate) fn start_wayvnc(&mut self) {
        if self.wayvnc_config.is_none() {
            return;
        }
        if let Err(error) = self.start_wayvnc_inner() {
            warn!(%error, "WayVNC unavailable; physical desktop remains running");
            self.disable_wayvnc(&error.to_string());
        }
    }

    fn start_wayvnc_inner(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(config) = &self.wayvnc_config else {
            return Ok(());
        };
        self.wayvnc = Some(ManagedProcess::spawn(
            config.spec(self.child_env(true), &self.paths.session_dir),
            0,
        )?);
        self.record_processes()?;
        let deadline = self
            .startup_deadline
            .min(Instant::now() + Duration::from_secs(5));
        loop {
            if self.shutdown_requested.load(Ordering::SeqCst) || Instant::now() >= deadline {
                return Err("WayVNC startup interrupted or timed out".into());
            }
            if let Some(status) = poll_process(&mut self.wayvnc)? {
                return Err(format!("WayVNC exited before readiness: {status}").into());
            }
            let budget = deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100));
            // The private control socket distinguishes this child from an existing
            // VNC server that already occupies the requested TCP port.
            if self.paths.session_dir.join("wayvnc-control.sock").exists()
                && !budget.is_zero()
                && config.ready(budget)
            {
                if let Some(status) = poll_process(&mut self.wayvnc)? {
                    return Err(format!("WayVNC exited during readiness: {status}").into());
                }
                self.record_wayvnc(true, None)?;
                info!(
                    address = "127.0.0.1",
                    port = config.port,
                    "WayVNC ready; use an SSH tunnel to connect remotely"
                );
                return Ok(());
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    pub(super) fn poll_wayvnc(&mut self) {
        match poll_process(&mut self.wayvnc) {
            Ok(Some(status)) => {
                warn!(%status, "WayVNC exited; physical desktop remains running");
                self.disable_wayvnc(&format!("WayVNC exited: {status}"));
            }
            Err(error) => {
                warn!(%error, "failed to monitor WayVNC; disabling remote sharing");
                self.disable_wayvnc(&error.to_string());
            }
            Ok(None) => {}
        }
    }

    fn disable_wayvnc(&mut self, reason: &str) {
        terminate_process(&mut self.wayvnc);
        wait_then_kill_process(&mut self.wayvnc);
        self.wayvnc = None;
        if let Err(error) = self
            .record_processes()
            .and_then(|()| self.record_wayvnc(false, Some(reason)))
        {
            warn!(%error, "failed to record WayVNC state");
        }
    }

    fn record_wayvnc(&self, ready: bool, error: Option<&str>) -> std::io::Result<()> {
        let port = self.wayvnc_config.as_ref().map(|config| config.port);
        let record = serde_json::json!({
            "ready": ready, "address": "127.0.0.1", "port": port, "error": error,
        });
        fs::write(
            self.paths.session_dir.join("wayvnc.json"),
            record.to_string(),
        )
    }
}
