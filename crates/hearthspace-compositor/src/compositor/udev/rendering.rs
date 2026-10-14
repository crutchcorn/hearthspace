use super::UdevBackendState;
use crate::compositor::capture::{CaptureState, encode_png, read_pixels};
use smithay::{
    backend::{drm::DrmEventMetadata, renderer::Bind},
    reexports::drm::control::crtc,
    utils::Rectangle,
};
use tracing::{debug, trace, warn};

impl UdevBackendState {
    pub(in crate::compositor) fn capture_active(&self) -> bool {
        self.session_active
            && !self.drm_commits_paused
            && self
                .primary_device
                .as_ref()
                .is_some_and(|device| device.has_output_surfaces())
    }

    pub(in crate::compositor) fn render_frame(
        &mut self,
        state: &mut crate::compositor::App,
        force_full_redraw: bool,
        captures: &mut CaptureState,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if state.session_locked() {
            captures.invalidate("session is locked");
        }
        if self.drm_commits_paused {
            debug!("skipping native render while DRM commits are paused");
            self.frame_dirty = true;
            if let Some(device) = &mut self.primary_device {
                device.mark_all_surfaces_dirty();
            }
            return Ok(false);
        }

        let Some(device) = self.primary_device.as_mut() else {
            return Err("udev backend has no primary DRM device".into());
        };
        if device.output_surfaces.is_empty() {
            debug!("skipping native render because no KMS output surface is initialized");
            self.frame_dirty = true;
            return Ok(false);
        }

        let cursor = device.capture_cursor(state);
        let renderer = &mut device.render_node.renderer;
        let mut submitted = false;
        for output in &mut device.output_surfaces {
            if output.frame_pending {
                output.frame_dirty = true;
                debug!(crtc = ?output.gbm_surface.crtc(), "native redraw requested while output page flip is pending; deferring until vblank");
                continue;
            }

            let Some(view) = state.output_render_view(output.target.connector_name()) else {
                output.frame_dirty = true;
                warn!(
                    connector = ?output.target.connector,
                    name = output.target.connector_name(),
                    "skipping native output render because no Wayland output view exists"
                );
                continue;
            };

            let (mut dmabuf, buffer_age) = output.gbm_surface.next_buffer()?;
            let age = if force_full_redraw || state.software_cursor_visible {
                0
            } else {
                usize::from(buffer_age)
            };
            let mut capture = None;
            let damage = {
                let mut framebuffer = renderer.bind(&mut dmabuf)?;
                let damage = state.render_frame_at(
                    renderer,
                    &mut framebuffer,
                    &mut output.damage_tracker,
                    age,
                    Rectangle::new(view.location, view.size),
                    view.scale,
                )?;
                if captures.wanted(&view.name) {
                    let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                        let size = output.target.output_size();
                        // Native GBM rows already follow scanout's top-down order.
                        // Nested EGL surfaces use Flipped180 and need a row reversal.
                        let mut pixels = read_pixels(renderer, &framebuffer, size, false)?;
                        let mut cursor_source = "framebuffer";
                        if let Some((crtc, cursor)) = &cursor
                            && *crtc == output.gbm_surface.crtc()
                        {
                            cursor.paint(&mut pixels, size);
                            cursor_source = "hardware-overlay";
                        }
                        let metadata = state
                            .capture_outputs()
                            .into_iter()
                            .find(|o| o.name == view.name)
                            .ok_or("capture output disappeared")?;
                        Ok((metadata, encode_png(size, &pixels)?, cursor_source))
                    })();
                    match result {
                        Ok(frame) => capture = Some(frame),
                        Err(error) => {
                            captures.fail(&view.name, &format!("native readback failed: {error}"))
                        }
                    }
                }
                damage
            };
            // Keep damage tracking for renderer-side redraw minimization, but do not
            // forward damage clips to KMS yet. Some virtual DRM stacks reject
            // FB_DAMAGE_CLIPS on page flip with EINVAL under client redraw load.
            let _damage = damage;
            if let Err(error) = output.gbm_surface.queue_buffer(None, None, ()) {
                captures.fail(&view.name, &format!("native presentation failed: {error}"));
                return Err(error.into());
            }
            if let Some((metadata, png, cursor_source)) = capture {
                captures.stage(metadata, png, cursor_source);
            }
            output.lock_frame_generation = state.lock_frame_generation();
            output.frame_pending = true;
            output.frame_dirty = false;
            submitted = true;
            trace!(
                crtc = ?output.gbm_surface.crtc(),
                name = %view.name,
                location = ?view.location,
                age,
                "queued native frame"
            );
        }

        self.frame_pending = device.any_frame_pending();
        self.frame_dirty = device.any_frame_dirty();
        self.repaint_pending = self.frame_pending;
        Ok(submitted)
    }

    pub(in crate::compositor) fn frame_submitted(
        &mut self,
        crtc: crtc::Handle,
        metadata: Option<&DrmEventMetadata>,
        state: &mut crate::compositor::App,
        captures: &mut CaptureState,
    ) -> Result<Option<bool>, Box<dyn std::error::Error>> {
        let Some(device) = self.primary_device.as_mut() else {
            return Ok(None);
        };
        let Some(output) = device
            .output_surfaces
            .iter_mut()
            .find(|output| output.gbm_surface.crtc() == crtc)
        else {
            return Ok(None);
        };
        if !output.event_cutoff.accepts(metadata) {
            trace!(
                ?crtc,
                ?metadata,
                "ignoring DRM event predating the current surface"
            );
            return Ok(None);
        }
        if output.gbm_surface.frame_submitted()?.is_none() {
            // A late event can outlive its surface after output reconfiguration.
            // Only acknowledge a frame that this surface actually submitted.
            return Ok(None);
        }
        if state.session_locked() {
            captures.invalidate("session is locked");
        } else {
            captures.presented(output.target.connector_name(), "page-flip");
        }
        if let Some(generation) = output.lock_frame_generation.take() {
            state.lock_frame_presented(output.target.connector_name(), generation);
        }
        output.frame_pending = false;
        self.frame_pending = device.any_frame_pending();
        self.repaint_pending = self.frame_pending;
        self.frame_dirty = device.any_frame_dirty();
        if self.frame_pending {
            return Ok(None);
        }

        let should_redraw = self.frame_dirty;
        if should_redraw {
            debug!("native page flip completed with dirty state; scheduling next frame");
        }
        Ok(Some(should_redraw))
    }
}
