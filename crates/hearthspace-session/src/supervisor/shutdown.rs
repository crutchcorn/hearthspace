use super::*;

impl Supervisor {
    pub(crate) fn shutdown(&mut self) -> std::io::Result<()> {
        info!("shutting down Hearthspace session processes");
        self.ai.take();
        self.power.take();
        terminate_process(&mut self.wayvnc);
        terminate_process(&mut self.shell);
        terminate_process(&mut self.compositor);
        terminate_process(&mut self.settingsd);
        terminate_process(&mut self.watcher);
        terminate_process(&mut self.notifications);

        self.desktop_services.shutdown();
        self.appearance_portal.take();

        wait_then_kill_process(&mut self.wayvnc);
        wait_then_kill_process(&mut self.shell);
        wait_then_kill_process(&mut self.compositor);
        wait_then_kill_process(&mut self.settingsd);
        wait_then_kill_process(&mut self.watcher);
        wait_then_kill_process(&mut self.notifications);

        let retired = match &mut self.activation_environment {
            Some(activation) => activation.retire(),
            None => crate::session_bus::retire_persisted(&self.paths.session_dir),
        };
        self.activation_environment.take();
        // Startup records intent before the manager call, so cleanup must also
        // cover a failed start that never returned an in-memory owner.
        let scopes =
            crate::app_scopes::shutdown(&self.paths.session_dir, &self.paths.host_runtime_dir);
        // Always reap our broker, including when a failed barrier requires the
        // runtime and recovery records to remain. Supplied buses are not owned.
        terminate_process(&mut self.session_bus);
        wait_then_kill_process(&mut self.session_bus);
        self.session_bus = None;
        scopes?;
        retired?;

        // Do not traverse/remove a still-mounted desktop filesystem on failure.
        crate::portal_cleanup::unmount_runtime(&self.paths.session_dir)?;
        match fs::remove_dir_all(&self.paths.session_dir) {
            Ok(()) => {
                info!(path = %self.paths.session_dir.display(), "removed session runtime directory")
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        Ok(())
    }
}
