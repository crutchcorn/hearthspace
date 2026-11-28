//! Generic EWMH overlay classification; titles and classes are not policy inputs.
use super::*;

impl XState {
    pub(super) fn screen_overlay_role(
        &self,
        server_state: &super::super::RealServerState,
        window: x::Window,
        heuristics: &WindowRoleHeuristics,
        state: &x::GetPropertyReply,
    ) -> XResult<Option<WindowRole>> {
        let states = if state.format() == 32 && state.bytes_after() == 0 {
            state.value::<x::Atom>()
        } else {
            &[]
        };
        // EWMH window types are ordered by preference; skip only unknown types.
        let supported = [
            self.window_atoms.desktop,
            self.window_atoms.dock,
            self.window_atoms.toolbar,
            self.window_atoms.normal,
            self.window_atoms.dialog,
            self.window_atoms.drag_n_drop,
            self.window_atoms.splash,
            self.window_atoms.menu,
            self.window_atoms.popup_menu,
            self.window_atoms.dropdown_menu,
            self.window_atoms.utility,
            self.window_atoms.tooltip,
            self.window_atoms.combo,
            self.window_atoms.notification,
        ];
        let preferred = heuristics
            .window_types
            .iter()
            .find(|atom| supported.contains(atom));
        let notification = preferred == Some(&self.window_atoms.notification);
        let utility_overlay = heuristics.override_redirect
            && !heuristics.has_transient_for
            && preferred == Some(&self.window_atoms.utility)
            && [
                self.atoms.wm_above,
                self.atoms.wm_skip_taskbar,
                self.atoms.wm_skip_pager,
            ]
            .iter()
            .all(|atom| states.contains(atom));
        if server_state.supports_overlays() && (notification || utility_overlay) {
            let root_geometry = self
                .connection
                .wait_for_reply(self.connection.send_request(&x::GetGeometry {
                    drawable: x::Drawable::Window(self.root),
                }))?;
            let dimensions = self
                .connection
                .wait_for_reply(self.connection.send_request(&x::GetGeometry {
                    drawable: x::Drawable::Window(window),
                }))?;
            let x = (f64::from(dimensions.x()) + f64::from(dimensions.width()) / 2.)
                / f64::from(root_geometry.width().max(1));
            let y = (f64::from(dimensions.y()) + f64::from(dimensions.height()) / 2.)
                / f64::from(root_geometry.height().max(1));
            let role = server_state.overlay_role(x, y);
            debug!(
                "screen overlay {window:?}: root={}x{}, requested=({}, {}) {}x{}, role={role:?}",
                root_geometry.width(),
                root_geometry.height(),
                dimensions.x(),
                dimensions.y(),
                dimensions.width(),
                dimensions.height()
            );
            Ok(Some(role))
        } else {
            Ok(None)
        }
    }
}
