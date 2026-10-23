use super::*;

impl Supervisor {
    pub(crate) fn prepare_activation_environment(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self
            .session_env
            .vars
            .iter()
            .any(|(key, _)| key == "HEARTHSPACE_SESSION_BUS_ADDRESS")
        {
            self.activation_environment = crate::session_bus::update_activation_environment(
                &self.paths,
                &self.session_env,
                self.private_session_bus,
                self.startup_deadline,
                &self.shutdown_requested,
            )?;
        }
        Ok(())
    }
}
