//! Coordinate the one pointer across device-local legacy planes and submitted
//! software images. Completion history belongs to each existing KMS output.
use smithay::{
    backend::renderer::utils::with_renderer_surface_state,
    utils::{IsAlive, Rectangle},
};

use super::{
    UdevBackendState,
    device::cursor::policy::{ImageKey, Scene, Target},
};
use crate::compositor::App;

pub(super) mod retry;

impl UdevBackendState {
    pub(in crate::compositor) fn update_hardware_cursor(&mut self, state: &mut App) {
        if self.drm_commits_paused {
            return;
        }
        for device in self
            .opened_devices
            .values_mut()
            .filter(|device| device.active)
        {
            device.initialize_cursor();
        }
        let owner = self.opened_devices.iter().find_map(|(id, device)| {
            if !device.active {
                return None;
            }
            device.output_surfaces.iter().find_map(|output| {
                let view = state.output_render_view(output.target.connector_name())?;
                Rectangle::new(view.location, view.size)
                    .contains(state.pointer_location.to_i32_floor())
                    .then(|| {
                        (
                            *id,
                            Target {
                                crtc: output.target.crtc,
                                image: ImageKey {
                                    icon: state.cursor_icon,
                                    scale: view.scale.max(1),
                                },
                                local: (
                                    (state.pointer_location.x - f64::from(view.location.x))
                                        * f64::from(view.scale.max(1)),
                                    (state.pointer_location.y - f64::from(view.location.y))
                                        * f64::from(view.scale.max(1)),
                                ),
                            },
                            output.presentation.cursor.displayed,
                        )
                    })
            })
        });
        let scene = Scene {
            software_displayed: self
                .opened_devices
                .values()
                .filter(|d| d.active)
                .flat_map(|d| &d.output_surfaces)
                .any(|o| o.presentation.cursor.displayed),
            software_may_arrive: self
                .opened_devices
                .values()
                .filter(|d| d.active)
                .flat_map(|d| &d.output_surfaces)
                .any(|o| o.presentation.cursor.may_arrive()),
            replacement_displayed: owner.is_some_and(|(_, _, displayed)| displayed),
        };
        // A live wl_pointer cursor surface with no committed root buffer is an
        // intentional hidden image. Failed import of an existing buffer is
        // different: retain hardware until an actual rendered replacement.
        let empty_client = state.cursor_surface.as_ref().is_some_and(|surface| {
            surface.alive()
                && !with_renderer_surface_state(surface, |data| data.buffer().is_some())
                    .unwrap_or(false)
        });
        if !state.cursor_visible || empty_client || owner.is_none() {
            for device in self.opened_devices.values_mut().filter(|d| d.active) {
                device.cursor_plane.allow_retry();
                device.clear_hardware_cursor();
            }
            apply_visibility(state, false, scene);
            self.schedule_cursor_retry();
            return;
        }
        let (owner, target, _) = owner.expect("visible pointer has an output");
        let client = state.cursor_surface.is_some();
        let device = self
            .opened_devices
            .get_mut(&owner)
            .expect("selected device");
        // Returning from a client-owned cursor is an explicit new hardware
        // request. Otherwise a failed image/CRTC stays software until the named
        // image, scale, output, or device lifecycle changes; no per-loop retry.
        if client {
            device.cursor_plane.allow_retry();
        }
        let software = client || device.cursor_requires_software(target);
        let software = device.update_cursor(target, software, scene);
        let replacement_hardware = !software
            && !scene.software_displayed
            && !scene.software_may_arrive
            && device
                .cursor_plane
                .active
                .is_some_and(|active| active.crtc == target.crtc);

        // During cross-device handoff the old device keeps its cursor until the
        // new output presents software or successfully establishes hardware.
        for (id, device) in &mut self.opened_devices {
            if *id != owner && device.active {
                device.retire_previous_cursor(scene, replacement_hardware);
            }
        }
        apply_visibility(state, software, scene);
        self.schedule_cursor_retry();
    }
}

pub(super) fn apply_visibility(state: &mut App, software: bool, scene: Scene) {
    if state.software_cursor_visible != software {
        tracing::trace!(
            software,
            displayed = scene.software_displayed,
            pending = scene.software_may_arrive,
            "native cursor presentation requested"
        );
        state.software_cursor_visible = software;
        state.request_redraw();
    } else if !software && (scene.software_displayed || scene.software_may_arrive) {
        // Superseding a submitted software image cannot cancel that flip. Its
        // eventual completion still needs a fresh cursor-free scene.
        state.request_redraw();
    }
}
