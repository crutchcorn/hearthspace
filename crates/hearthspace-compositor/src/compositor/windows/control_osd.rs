//! Passive feedback has an independent lifetime from the focused shell popup.
use crate::compositor::{App, ManagedWindowKind};
use serde_json::{Value, json};

impl App {
    pub(in crate::compositor) fn control_osds_status(&self) -> Vec<Value> {
        if self.session_locked() {
            return Vec::new();
        }
        self.windows
            .iter()
            .enumerate()
            .filter(|(_, window)| window.mapped && window.kind == ManagedWindowKind::ControlOsd)
            .filter_map(|(index, window)| {
                let origin =
                    self.launcher_logical_origin(index)? + self.window_surface_geometry_loc(index);
                let size = self.window_geometry_size(index);
                Some(json!({"output": window.shell_output_name,
                "x": origin.x, "y": origin.y, "width": size.w, "height": size.h}))
            })
            .collect()
    }
}
