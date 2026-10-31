use smithay::{
    backend::input::InputTime,
    backend::renderer::utils::with_renderer_surface_state,
    utils::SERIAL_COUNTER,
    wayland::{
        compositor::{get_parent, with_states},
        session_lock::{LockSurface, LockSurfaceCachedState},
    },
};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::compositor::{
    App, focus::KeyboardFocusTarget, frame_callbacks::send_frames_surface_tree,
};

pub(super) fn ready(surface: &LockSurface) -> bool {
    surface.alive()
        && with_renderer_surface_state(surface.wl_surface(), |state| state.buffer().is_some())
            .unwrap_or(false)
        && with_states(surface.wl_surface(), |states| {
            states
                .cached_state
                .get::<LockSurfaceCachedState>()
                .current()
                .last_acked
                .is_some()
        })
}

fn belongs_to(surface: &WlSurface, roots: &[WlSurface]) -> bool {
    let mut root = surface.clone();
    while let Some(parent) = get_parent(&root) {
        root = parent;
    }
    roots.contains(&root)
}

impl App {
    pub(in crate::compositor) fn lock_surface_committed(&mut self, root: &WlSurface) {
        if !self.session_locked() {
            return;
        }
        if self
            .session_lock
            .surfaces
            .iter()
            .any(|(_, surface)| surface.wl_surface() == root && ready(surface))
        {
            if self.keyboard.current_focus().is_none() {
                self.keyboard.clone().set_focus(
                    self,
                    Some(KeyboardFocusTarget::Wayland(root.clone())),
                    SERIAL_COUNTER.next_serial(),
                );
            }
            self.request_redraw();
        }
    }

    pub(in crate::compositor) fn maintain_session_lock(&mut self) {
        if !self.session_locked() {
            return;
        }
        let previous_count = self.session_lock.surfaces.len();
        if !self
            .session_lock
            .owner
            .as_ref()
            .is_some_and(Resource::is_alive)
        {
            self.session_lock.surfaces.clear();
        } else {
            self.session_lock
                .surfaces
                .retain(|(_, surface)| surface.alive());
        }
        let ready_surfaces: Vec<_> = self
            .session_lock
            .surfaces
            .iter()
            .filter(|(_, surface)| ready(surface))
            .map(|(_, surface)| surface.wl_surface().clone())
            .collect();
        let readiness_changed = ready_surfaces != self.session_lock.ready_surfaces;
        let focus_valid = self
            .keyboard
            .current_focus()
            .is_some_and(|focus| belongs_to(focus.surface(), &ready_surfaces));
        if !focus_valid && self.keyboard.current_focus().is_some() {
            self.keyboard.clone().unset_grab(self);
            self.clear_keyboard_focus();
        }
        if !focus_valid && let Some(surface) = ready_surfaces.first() {
            self.keyboard.clone().set_focus(
                self,
                Some(KeyboardFocusTarget::Wayland(surface.clone())),
                SERIAL_COUNTER.next_serial(),
            );
        }
        if readiness_changed {
            if self
                .pointer
                .current_focus()
                .is_some_and(|surface| !belongs_to(&surface, &ready_surfaces))
            {
                self.pointer.clone().unset_grab(
                    self,
                    SERIAL_COUNTER.next_serial(),
                    InputTime::now(),
                );
                self.cursor_surface = None;
            }
            self.session_lock.ready_surfaces = ready_surfaces;
            self.lock_pointer_motion(InputTime::now());
        }
        if readiness_changed || previous_count != self.session_lock.surfaces.len() {
            // Destroying only the lock role leaves wl_surface alive. Its
            // cached configure disappears, so explicitly repaint the opaque
            // fallback even when no buffer commit or window removal occurs.
            self.request_redraw();
        }
    }

    pub(in crate::compositor) fn lock_frame_callback_surfaces(
        &self,
    ) -> Option<&[(String, LockSurface)]> {
        // Some(empty) still suppresses ordinary clients while the lock client
        // is starting, has disconnected, or has not created its surfaces yet.
        self.session_locked()
            .then_some(self.session_lock.surfaces.as_slice())
    }
}

pub(in crate::compositor) fn send_lock_frame_callbacks(
    surfaces: &[(String, LockSurface)],
    output: Option<&str>,
    time: u32,
) -> usize {
    surfaces
        .iter()
        .filter(|(name, surface)| output.is_none_or(|output| name == output) && ready(surface))
        .map(|(_, surface)| send_frames_surface_tree(surface.wl_surface(), time))
        .sum()
}
