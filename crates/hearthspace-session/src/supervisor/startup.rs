//! Startup is complete only after the shell's committed desktop is presented.
use super::*;

impl Supervisor {
    pub(super) fn wait_for_desktop_ready(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let native = match self.backend {
            Some(Backend::Native) => true,
            Some(Backend::Nested) => false,
            None => select_compositor_backend() == CompositorBackend::NativeTty,
        };
        info!("waiting for mapped shell and desktop presentation");
        let mut last_error = None;
        loop {
            if let Err(error) = self.check_startup_deadline("desktop presentation") {
                return Err(format!("{error}; last readiness response: {last_error:?}").into());
            }
            for (name, process) in [
                ("shell", &mut self.shell),
                ("compositor", &mut self.compositor),
                ("settingsd", &mut self.settingsd),
            ] {
                if let Some(status) = poll_process(process)? {
                    return Err(format!("{name} exited before desktop readiness: {status}").into());
                }
            }
            let deadline = self
                .startup_deadline
                .min(Instant::now() + Duration::from_secs(3));
            match crate::readiness::desktop::probe(
                &self.paths.shell_command_socket,
                native,
                deadline,
                &self.shutdown_requested,
            ) {
                Ok(true) => {
                    info!("desktop ready: shell mapped and every output presented");
                    return Ok(());
                }
                Ok(false) => last_error = Some("desktop is not yet mapped and active".to_owned()),
                Err(error) => last_error = Some(error.to_string()),
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}
