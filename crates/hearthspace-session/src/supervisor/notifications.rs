use super::*;

impl Supervisor {
    pub(crate) fn start_notifications(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let spec = self.binary_spec("hearthspace-notifications", &[], true);
        self.notifications = Some(ManagedProcess::spawn(spec, 2)?);
        self.record_processes()?;
        let socket = self
            .paths
            .session_dir
            .join("hearthspace-notifications.sock");
        loop {
            self.check_startup_deadline("notifications")?;
            if let Some(status) = poll_process(&mut self.notifications)? {
                return Err(format!("notifications exited before readiness: {status}").into());
            }
            if crate::readiness::connect_until(
                &socket,
                self.startup_deadline
                    .min(Instant::now() + Duration::from_millis(100)),
                &self.shutdown_requested,
            )
            .is_ok()
            {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}
