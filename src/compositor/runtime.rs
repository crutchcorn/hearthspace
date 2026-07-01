use std::time::Instant;

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Bind, ExportMem, ImportDma,
            damage::OutputDamageTracker,
            gles::{GlesRenderbuffer, GlesRenderer},
        },
    },
    desktop::PopupManager,
    reexports::{calloop::EventLoop, wayland_server::Display},
    utils::{Buffer as BufferCoord, Physical, Rectangle, Size, Transform},
};
use tracing::{debug, error, info, trace, warn};

use super::{
    ANIMATION_FRAME_INTERVAL, App, cursor::CursorIcon, rendering::send_frames_surface_tree,
};

pub(in crate::compositor) enum Backend {
    #[cfg(feature = "winit")]
    Winit(Box<smithay::backend::winit::WinitGraphicsBackend<GlesRenderer>>),
    Headless(Box<HeadlessBackend>),
    #[cfg(feature = "udev")]
    #[allow(dead_code)]
    Udev(Box<super::udev::UdevBackendState>),
}

pub(in crate::compositor) struct HeadlessBackend {
    pub(in crate::compositor) renderer: GlesRenderer,
    pub(in crate::compositor) buffer: GlesRenderbuffer,
}

pub(in crate::compositor) struct CalloopData {
    pub(in crate::compositor) state: App,
    pub(in crate::compositor) display: Display<App>,
    pub(in crate::compositor) backend: Backend,
    pub(in crate::compositor) damage_tracker: OutputDamageTracker,
    pub(in crate::compositor) start_time: Instant,
    pub(in crate::compositor) running: bool,
    pub(in crate::compositor) exit_at: Option<Instant>,
    // Number of upcoming frames that must be fully redrawn instead of querying
    // the back buffer age. Importing a client dmabuf (or the first frame) can
    // leave the renderer's EGL context surfaceless, which makes `buffer_age`
    // (an `eglQuerySurface` that requires the window surface be current) fail.
    pub(in crate::compositor) full_redraw: u8,
    // Cursor icon currently applied to the winit window, so the desired cursor
    // (`state.cursor_icon`) is only pushed to the backend when it changes.
    pub(in crate::compositor) applied_cursor: CursorIcon,
}

pub(in crate::compositor) fn create_headless_calloop_data(
    state: App,
    display: Display<App>,
    backend: HeadlessBackend,
    output_size: Size<i32, Physical>,
    exit_after: Option<std::time::Duration>,
) -> CalloopData {
    let start_time = Instant::now();
    debug!(
        ?output_size,
        ?exit_after,
        "creating headless event-loop data"
    );
    CalloopData {
        state,
        display,
        backend: Backend::Headless(Box::new(backend)),
        damage_tracker: OutputDamageTracker::new(output_size, 1.0, Transform::Flipped180),
        start_time,
        running: true,
        exit_at: exit_after.map(|duration| start_time + duration),
        full_redraw: 1,
        applied_cursor: CursorIcon::Default,
    }
}

#[cfg(feature = "udev")]
pub(in crate::compositor) fn create_calloop_data(
    state: App,
    display: Display<App>,
    backend: Backend,
    output_size: Size<i32, Physical>,
    exit_after: Option<std::time::Duration>,
) -> CalloopData {
    let start_time = Instant::now();
    debug!(
        ?output_size,
        ?exit_after,
        "creating compositor event-loop data"
    );
    CalloopData {
        state,
        display,
        backend,
        damage_tracker: OutputDamageTracker::new(output_size, 1.0, Transform::Normal),
        start_time,
        running: true,
        exit_at: exit_after.map(|duration| start_time + duration),
        full_redraw: 1,
        applied_cursor: CursorIcon::Default,
    }
}

pub(in crate::compositor) fn run_event_loop(
    mut event_loop: EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("entering compositor event loop");
    if let Err(error) = data.render() {
        warn!(%error, "initial compositor render failed");
    }
    data.state.needs_redraw = false;

    while data.running {
        // Block until an event arrives; while animating, wake every frame.
        let animation_timeout = (!data.uses_vblank_animation_pacing())
            .then_some(())
            .and_then(|()| {
                data.state
                    .viewport_animation
                    .is_some()
                    .then_some(ANIMATION_FRAME_INTERVAL)
            });
        let exit_timeout = data
            .exit_at
            .map(|exit_at| exit_at.saturating_duration_since(Instant::now()));
        let timeout = match (animation_timeout, exit_timeout) {
            (Some(animation), Some(exit)) => Some(animation.min(exit)),
            (Some(animation), None) => Some(animation),
            (None, Some(exit)) => Some(exit),
            (None, None) => None,
        };
        event_loop.dispatch(timeout, data)?;

        if data
            .exit_at
            .is_some_and(|exit_at| Instant::now() >= exit_at)
        {
            info!("exit timer elapsed; stopping compositor event loop");
            data.running = false;
        }

        if !data.running {
            break;
        }

        data.process_pending_dmabuf_imports();
        data.state.handle_idle_transitions();
        data.state.advance_viewport_animation();
        data.apply_cursor_icon();

        if data.state.needs_redraw {
            if let Err(error) = data.render() {
                warn!(%error, "compositor render failed; dropping this redraw");
                data.full_redraw = data.full_redraw.max(1);
            }
            data.state.needs_redraw = false;
        }

        if let Err(error) = data.display.flush_clients() {
            error!(%error, "failed to flush Wayland clients");
        }
        data.state.popups.cleanup();
        data.state.cleanup_outputs();
    }

    info!("compositor event loop exited");
    Ok(())
}

impl CalloopData {
    fn uses_vblank_animation_pacing(&self) -> bool {
        match &self.backend {
            #[cfg(feature = "udev")]
            Backend::Udev(_) => true,
            #[cfg(feature = "winit")]
            Backend::Winit(_) | Backend::Headless(_) => false,
            #[cfg(not(feature = "winit"))]
            Backend::Headless(_) => false,
        }
    }

    /// Push the compositor's desired cursor to the host winit window, but only
    /// when it differs from the cursor currently shown.
    fn apply_cursor_icon(&mut self) {
        if self.applied_cursor == self.state.cursor_icon {
            return;
        }
        trace!(from = ?self.applied_cursor, to = ?self.state.cursor_icon, "applying cursor icon");
        self.applied_cursor = self.state.cursor_icon;
        #[cfg(feature = "winit")]
        if let Backend::Winit(backend) = &self.backend {
            backend.window().set_cursor(self.applied_cursor);
        }
    }

    fn process_pending_dmabuf_imports(&mut self) {
        if self.state.pending_dmabuf_imports.is_empty() {
            return;
        }
        debug!(
            count = self.state.pending_dmabuf_imports.len(),
            "processing pending client dmabuf imports"
        );
        let CalloopData { state, backend, .. } = self;
        for (dmabuf, notifier) in state.pending_dmabuf_imports.drain(..) {
            let import = match backend {
                #[cfg(feature = "winit")]
                Backend::Winit(backend) => backend.renderer().import_dmabuf(&dmabuf, None),
                Backend::Headless(backend) => backend.renderer.import_dmabuf(&dmabuf, None),
                #[cfg(feature = "udev")]
                Backend::Udev(backend) => {
                    match backend.import_dmabuf(&dmabuf) {
                        Ok(()) => {
                            let _ = notifier.successful::<App>();
                        }
                        Err(error) => {
                            warn!(%error, "failed to import client dmabuf");
                            notifier.failed();
                        }
                    }
                    continue;
                }
            };
            match import {
                Ok(_texture) => {
                    let _ = notifier.successful::<App>();
                }
                Err(error) => {
                    warn!(%error, "failed to import client dmabuf");
                    notifier.failed();
                }
            }
        }
        // Importing made the renderer's EGL context surfaceless, so skip the
        // next frame's back-buffer-age query and redraw it fully instead.
        self.full_redraw = self.full_redraw.max(1);
    }

    fn render(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        trace!(full_redraw = self.full_redraw, "rendering compositor frame");
        let send_callbacks_now = match &mut self.backend {
            #[cfg(feature = "winit")]
            Backend::Winit(backend) => {
                // `buffer_age` is an `eglQuerySurface` that only succeeds while
                // the window surface is the current EGL draw surface. After a
                // dmabuf import (or on the first frame) that is not guaranteed,
                // so those frames are forced to a full redraw (age 0) instead of
                // querying a stale surface.
                let age = if self.full_redraw > 0 {
                    self.full_redraw = self.full_redraw.saturating_sub(1);
                    0
                } else {
                    backend.buffer_age().unwrap_or(0)
                };
                let damage = {
                    let (renderer, mut framebuffer) = backend.bind()?;
                    self.state.render_frame(
                        renderer,
                        &mut framebuffer,
                        &mut self.damage_tracker,
                        age,
                    )?
                };

                trace!(
                    damage_regions = damage.as_ref().map_or(0, Vec::len),
                    "rendered winit frame"
                );
                if let Some(damage) = damage.as_ref() {
                    backend.submit(Some(damage))?;
                }
                true
            }
            Backend::Headless(backend) => {
                self.full_redraw = 0;
                let mut framebuffer = backend.renderer.bind(&mut backend.buffer)?;
                self.state.render_frame(
                    &mut backend.renderer,
                    &mut framebuffer,
                    &mut self.damage_tracker,
                    0,
                )?;
                trace!("rendered headless frame");
                true
            }
            #[cfg(feature = "udev")]
            Backend::Udev(backend) => {
                let force_full_redraw = self.full_redraw > 0;
                let submitted = backend.render_frame(&mut self.state, force_full_redraw)?;
                trace!(submitted, force_full_redraw, "rendered native frame");
                if submitted && force_full_redraw {
                    self.full_redraw = self.full_redraw.saturating_sub(1);
                }
                false
            }
        };

        if send_callbacks_now && let Err(error) = self.send_frame_callbacks() {
            warn!(%error, "failed to send frame callbacks");
        }

        Ok(())
    }

    pub(in crate::compositor) fn send_frame_callbacks(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for window in &self.state.windows {
            send_frames_surface_tree(
                window.surface.wl_surface(),
                self.start_time.elapsed().as_millis() as u32,
            );
            // Popups (e.g. client menus) are tracked separately from the window
            // surface tree, so they need their own frame callbacks. Without
            // these the client (e.g. GTK4) throttles and never repaints the
            // popup after its first frame, so keyboard navigation highlights
            // never appear.
            for (popup, _) in PopupManager::popups_for_surface(window.surface.wl_surface()) {
                send_frames_surface_tree(
                    popup.wl_surface(),
                    self.start_time.elapsed().as_millis() as u32,
                );
            }
        }

        self.display.flush_clients()?;

        Ok(())
    }

    pub(in crate::compositor) fn screenshot_png(
        &mut self,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        debug!("capturing compositor screenshot");
        self.process_pending_dmabuf_imports();

        let CalloopData { state, backend, .. } = self;
        let size = state.output_size();
        let mut screenshot_damage = OutputDamageTracker::new(size, 1.0, Transform::Flipped180);
        let region = Rectangle::from_size(Size::<i32, BufferCoord>::from((size.w, size.h)));
        let pixels = match backend {
            #[cfg(feature = "winit")]
            Backend::Winit(backend) => {
                let (renderer, mut framebuffer) = backend.bind()?;
                state.render_frame(renderer, &mut framebuffer, &mut screenshot_damage, 0)?;
                let mapping = renderer.copy_framebuffer(&framebuffer, region, Fourcc::Abgr8888)?;
                renderer.map_texture(&mapping)?.to_vec()
            }
            Backend::Headless(backend) => {
                let mut framebuffer = backend.renderer.bind(&mut backend.buffer)?;
                state.render_frame(
                    &mut backend.renderer,
                    &mut framebuffer,
                    &mut screenshot_damage,
                    0,
                )?;
                let mapping =
                    backend
                        .renderer
                        .copy_framebuffer(&framebuffer, region, Fourcc::Abgr8888)?;
                backend.renderer.map_texture(&mapping)?.to_vec()
            }
            #[cfg(feature = "udev")]
            Backend::Udev(_) => {
                return Err(
                    "screenshots are unsupported on the udev backend until native readback is implemented"
                        .into(),
                );
            }
        };
        let png = encode_png_rgba(size, &pixels)?;
        debug!(?size, bytes = png.len(), "captured compositor screenshot");
        Ok(png)
    }
}

fn encode_png_rgba(
    size: Size<i32, Physical>,
    bottom_up_rgba: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let width = usize::try_from(size.w)?;
    let height = usize::try_from(size.h)?;
    let stride = width.checked_mul(4).ok_or("screenshot stride overflow")?;
    let expected_len = stride
        .checked_mul(height)
        .ok_or("screenshot buffer length overflow")?;
    if bottom_up_rgba.len() != expected_len {
        return Err(format!(
            "screenshot readback returned {} bytes, expected {expected_len}",
            bottom_up_rgba.len()
        )
        .into());
    }

    let mut top_down_rgba = Vec::with_capacity(expected_len);
    for row in bottom_up_rgba.chunks_exact(stride).rev() {
        top_down_rgba.extend_from_slice(row);
    }

    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(
            &mut png_bytes,
            u32::try_from(width)?,
            u32::try_from(height)?,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&top_down_rgba)?;
    }
    Ok(png_bytes)
}
