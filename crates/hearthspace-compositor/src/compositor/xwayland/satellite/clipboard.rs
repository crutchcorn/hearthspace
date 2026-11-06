//! Privileged selection export is restricted to this runtime's bridge client.
use crate::compositor::App;
use smithay::wayland::selection::ext_data_control::{DataControlHandler, DataControlState};
impl DataControlHandler for App {
    fn data_control_state(&mut self) -> &mut DataControlState {
        self.satellite_interaction
            .data_control
            .as_mut()
            .expect("private Satellite data control")
    }
}
