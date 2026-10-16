use std::time::Instant;

use super::{UdevBackendState, event_cutoff::SurfaceEvent};
use crate::compositor::capture::{CaptureState, read_pixels};
use smithay::{
    backend::{
        drm::{DrmEventMetadata, DrmEventTime},
        renderer::{Bind, damage::OutputDamageTracker},
    },
    reexports::drm::control::crtc,
    utils::{Clock, Monotonic, Rectangle, Transform},
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
            state.remote_capture.stop();
        }
        if self.drm_commits_paused {
            state.remote_capture.stop();
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
                // When scanout contains a software cursor, obtain a cursor-free
                // scene first for sessions that did not request PaintCursors.
                // The real scanout render below is always restored in full.
                let clean = if state.software_cursor_visible
                    && state.remote_capture.wants_cursorless(&view.name)
                {
                    let size = output.target.output_size();
                    let mut scratch =
                        OutputDamageTracker::new(size, f64::from(view.scale), Transform::Normal);
                    let visible = std::mem::replace(&mut state.software_cursor_visible, false);
                    let rendered = state.render_frame_at(
                        renderer,
                        &mut framebuffer,
                        &mut scratch,
                        0,
                        Rectangle::new(view.location, view.size),
                        view.scale,
                    );
                    state.software_cursor_visible = visible;
                    match rendered.and_then(|_| read_pixels(renderer, &framebuffer, size, false)) {
                        Ok(pixels) => Some(pixels),
                        Err(error) => {
                            warn!(%error, "cursor-free remote readback failed");
                            state.remote_capture.fail_output(&view.name);
                            None
                        }
                    }
                } else {
                    None
                };
                let damage = state.render_frame_at(
                    renderer,
                    &mut framebuffer,
                    &mut output.damage_tracker,
                    if clean.is_some() { 0 } else { age },
                    Rectangle::new(view.location, view.size),
                    view.scale,
                )?;
                if captures.wanted(&view.name) || state.remote_capture.wanted(&view.name) {
                    let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                        let size = output.target.output_size();
                        // Native GBM rows already follow scanout's top-down order.
                        // Nested EGL surfaces use Flipped180 and need a row reversal.
                        let mut pixels = read_pixels(renderer, &framebuffer, size, false)?;
                        let plain = clean.unwrap_or_else(|| pixels.clone());
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
                        Ok((metadata, size, plain, pixels, cursor_source))
                    })();
                    match result {
                        Ok(frame) => capture = Some(frame),
                        Err(error) => {
                            captures.fail(&view.name, &format!("native readback failed: {error}"));
                            state.remote_capture.fail_output(&view.name);
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
                state.remote_capture.fail_output(&view.name);
                return Err(error.into());
            }
            if let Some((metadata, size, plain, pixels, cursor_source)) = capture {
                if state.remote_capture.wanted(&view.name) {
                    state
                        .remote_capture
                        .stage(&view.name, size, plain, Some(pixels.clone()));
                }
                if captures.wanted(&view.name)
                    && let Err(error) = captures.stage_pixels(metadata, size, pixels, cursor_source)
                {
                    captures.fail(&view.name, &format!("native encoding failed: {error}"));
                }
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
        if !output.frame_pending {
            return Ok(None);
        }
        let event = output.event_cutoff.classify(metadata);
        if event == SurfaceEvent::Stale {
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
        if event == SurfaceEvent::Untimed {
            state
                .remote_capture
                .fail_output(output.target.connector_name());
            // A modeset completion without a timestamp can unblock the GBM
            // queue, but cannot prove this generation reached the display.
            // Discard its readback and lock marker; verify a fresh normal flip.
            captures.fail(
                output.target.connector_name(),
                "native presentation timestamp unavailable; retry capture",
            );
            output.lock_frame_generation = None;
            output.frame_pending = false;
            output.frame_dirty = true;
            self.frame_pending = device.any_frame_pending();
            self.repaint_pending = self.frame_pending;
            self.frame_dirty = device.any_frame_dirty();
            self.recovery_progress_at = Some(Instant::now());
            state.request_redraw();
            debug!(
                ?crtc,
                "untimed native modeset completed; requesting a verified page flip"
            );
            return Ok(None);
        }
        output.presented_once = true;
        if state.session_locked() {
            captures.invalidate("session is locked");
            state.remote_capture.stop();
        } else {
            captures.presented(output.target.connector_name(), "page-flip");
            if self.session_active && !self.drm_commits_paused {
                let timestamp = match metadata.map(|m| m.time) {
                    Some(DrmEventTime::Monotonic(time)) => time,
                    // The protocol requires CLOCK_MONOTONIC even on old DRM
                    // drivers reporting CLOCK_REALTIME page-flip timestamps.
                    _ => Clock::<Monotonic>::new().now().into(),
                };
                state
                    .remote_capture
                    .presented(output.target.connector_name(), timestamp);
            } else {
                state.remote_capture.stop();
            }
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
