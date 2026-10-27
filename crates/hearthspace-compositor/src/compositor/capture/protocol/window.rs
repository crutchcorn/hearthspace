//! Isolated window frames never read the output framebuffer.
use smithay::{
    backend::renderer::gles::GlesRenderer,
    utils::{Clock, Monotonic},
};

use super::{toplevel::WindowGeometry, *};
use crate::compositor::{CalloopData, runtime::Backend};

pub(super) struct WindowRequest {
    pub session: SessionRef,
    pub frame: Frame,
    pub geometry: WindowGeometry,
    pub deadline: Instant,
}

impl RemoteCaptureState {
    pub(super) fn request_window(
        &mut self,
        session: &SessionRef,
        frame: Frame,
        geometry: WindowGeometry,
    ) {
        if self.requests.len() + self.window_requests.len() >= MAX_SESSIONS {
            frame.fail(Failure::Unknown);
            return;
        }
        if session
            .current_constraints()
            .is_none_or(|c| c.size != geometry.size)
        {
            if let Some(constraints) = constraints(geometry.size) {
                session.update_constraints(constraints);
            }
            frame.fail(Failure::BufferConstraints);
            return;
        }
        self.window_requests.push(WindowRequest {
            session: session.clone(),
            frame,
            geometry,
            deadline: Instant::now() + FRAME_TIMEOUT,
        });
    }
}

impl App {
    pub(in crate::compositor) fn render_window_captures(&mut self, renderer: &mut GlesRenderer) {
        for request in std::mem::take(&mut self.remote_capture.window_requests) {
            if !self.capture_available()
                || !request.session.alive()
                || request.frame.has_failed()
                || request.deadline <= Instant::now()
            {
                request.frame.fail(Failure::Stopped);
                continue;
            }
            let source = request.session.source();
            let Some((index, geometry)) = self.capture_window_geometry(&source) else {
                request.frame.fail(Failure::Stopped);
                continue;
            };
            if geometry != request.geometry {
                if let Some(constraints) = constraints(geometry.size) {
                    request.session.update_constraints(constraints);
                }
                request.frame.fail(Failure::BufferConstraints);
                continue;
            }
            let result = self.render_capture_window(
                renderer,
                index,
                geometry,
                request.session.draw_cursor(),
            );
            // Render/readback is synchronous on the compositor thread. Recheck
            // admission and mapping before the only write to client memory.
            if !self.capture_available()
                || !request.session.alive()
                || request.frame.has_failed()
                || self.capture_window_geometry(&source).map(|(_, g)| g) != Some(geometry)
                || request.deadline <= Instant::now()
            {
                request.frame.fail(Failure::Stopped);
                continue;
            }
            match result {
                Ok(pixels) => match shm::copy_rgba(&request.frame.buffer(), geometry.size, &pixels)
                {
                    Ok(()) => request.frame.success(
                        Transform::Normal,
                        Some(vec![Rectangle::from_size(geometry.size)]),
                        Duration::from(Clock::<Monotonic>::new().now()),
                    ),
                    Err(reason) => request.frame.fail(reason),
                },
                Err(error) => {
                    tracing::warn!(%error, "window capture readback failed");
                    request.frame.fail(Failure::Unknown);
                }
            }
        }
    }
}

impl CalloopData {
    pub(in crate::compositor) fn capture_toplevels(&mut self) {
        if self.state.remote_capture.window_requests.is_empty() {
            return;
        }
        if !self.capture_active() || !self.state.capture_available() {
            self.state.remote_capture.stop();
            return;
        }
        self.process_pending_dmabuf_imports();
        match &mut self.backend {
            #[cfg(feature = "winit")]
            Backend::Winit(backend) => self.state.render_window_captures(backend.renderer()),
            Backend::Headless(backend) => self.state.render_window_captures(&mut backend.renderer),
            #[cfg(feature = "udev")]
            Backend::Udev(backend) => backend.capture_toplevels(&mut self.state),
        }
        // An offscreen bind may have made Winit's EGL context surfaceless.
        self.full_redraw = self.full_redraw.max(1);
    }
}
