//! Standard output capture, using Smithay's protocol validation and lifetime handling.
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use smithay::{
    output::{Output, WeakOutput},
    utils::{Buffer, IsAlive, Rectangle, Size, Transform},
    wayland::{
        image_capture_source::{
            ImageCaptureSource, ImageCaptureSourceHandler, OutputCaptureSourceHandler,
            OutputCaptureSourceState,
        },
        image_copy_capture::{
            BufferConstraints, CaptureFailureReason as Failure, Frame, FrameRef,
            ImageCopyCaptureHandler, ImageCopyCaptureState, Session, SessionRef,
        },
    },
};
use wayland_server::{DisplayHandle, protocol::wl_shm};

use super::super::App;

mod shm;
#[cfg(test)]
mod tests;

const MAX_SESSIONS: usize = 8;
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);

struct Request {
    session: SessionRef,
    frame: Frame,
    name: String,
    size: Size<i32, Buffer>,
    deadline: Instant,
    pixels: Option<Arc<[u8]>>,
}

pub(in crate::compositor) struct RemoteCaptureState {
    output_source: OutputCaptureSourceState,
    copy: ImageCopyCaptureState,
    sessions: Vec<Session>,
    requests: Vec<Request>,
}

impl RemoteCaptureState {
    pub(in crate::compositor) fn new(display: &DisplayHandle, enabled: bool) -> Self {
        Self::new_for::<App>(display, enabled)
    }

    fn new_for<D: OutputCaptureSourceHandler + ImageCopyCaptureHandler>(
        display: &DisplayHandle,
        enabled: bool,
    ) -> Self {
        Self {
            output_source: OutputCaptureSourceState::new_with_filter::<D, _>(display, move |_| {
                enabled
            }),
            copy: ImageCopyCaptureState::new_with_filter::<D, _>(display, move |_| enabled),
            sessions: Vec::new(),
            requests: Vec::new(),
        }
    }

    pub(in crate::compositor) fn timeout(&self) -> Option<Duration> {
        self.requests
            .iter()
            .map(|r| r.deadline.saturating_duration_since(Instant::now()))
            .min()
    }

    pub(in crate::compositor) fn wanted(&self, name: &str) -> bool {
        self.requests
            .iter()
            .any(|r| r.name == name && r.pixels.is_none())
    }

    #[cfg(feature = "udev")]
    pub(in crate::compositor) fn wants_cursorless(&self, name: &str) -> bool {
        self.requests
            .iter()
            .any(|r| r.name == name && r.pixels.is_none() && !r.session.draw_cursor())
    }

    pub(in crate::compositor) fn stage(
        &mut self,
        name: &str,
        size: Size<i32, smithay::utils::Physical>,
        plain: Vec<u8>,
        painted: Option<Vec<u8>>,
    ) {
        let plain: Arc<[u8]> = plain.into();
        let painted: Arc<[u8]> = painted.map(Into::into).unwrap_or_else(|| plain.clone());
        for request in &mut self.requests {
            if request.name == name
                && request.pixels.is_none()
                && request.size.w == size.w
                && request.size.h == size.h
            {
                request.pixels = Some(if request.session.draw_cursor() {
                    painted.clone()
                } else {
                    plain.clone()
                });
            }
        }
    }

    /// Do not touch client SHM until the matching output's verified page flip.
    pub(in crate::compositor) fn presented(&mut self, name: &str, time: Duration) {
        let mut pending = Vec::new();
        for request in self.requests.drain(..) {
            if request.name != name || request.pixels.is_none() {
                pending.push(request);
                continue;
            }
            if !request.session.alive()
                || request.frame.has_failed()
                || request.deadline <= Instant::now()
            {
                request.frame.fail(Failure::Stopped);
                continue;
            }
            let current_size = source_output(&request.session.source())
                .and_then(|output| output.current_mode())
                .map(|mode| (mode.size.w, mode.size.h));
            if current_size != Some((request.size.w, request.size.h)) {
                request.frame.fail(Failure::BufferConstraints);
                continue;
            }
            let pixels = request.pixels.unwrap();
            match shm::copy_rgba(&request.frame.buffer(), request.size, &pixels) {
                Ok(()) => request.frame.success(
                    Transform::Normal,
                    Some(vec![Rectangle::from_size(request.size)]),
                    time,
                ),
                Err(reason) => request.frame.fail(reason),
            }
        }
        self.requests = pending;
    }

    pub(in crate::compositor) fn fail_output(&mut self, name: &str) {
        self.requests.retain(|request| request.name != name);
    }

    pub(in crate::compositor) fn stop(&mut self) {
        // Dropping sessions sends stopped and fails their frames before releasing pixels.
        self.sessions.clear();
        self.requests.clear();
    }

    fn request(&mut self, session: &SessionRef, frame: Frame, name: String) {
        let Some(constraints) = session.current_constraints() else {
            frame.fail(Failure::Stopped);
            return;
        };
        if self.requests.len() >= MAX_SESSIONS {
            frame.fail(Failure::Unknown);
            return;
        }
        self.requests.push(Request {
            session: session.clone(),
            frame,
            name,
            size: constraints.size,
            deadline: Instant::now() + FRAME_TIMEOUT,
            pixels: None,
        });
    }
}

fn source_output(source: &ImageCaptureSource) -> Option<Output> {
    source.user_data().get::<WeakOutput>()?.upgrade()
}

fn constraints(size: Size<i32, Buffer>) -> Option<BufferConstraints> {
    super::pixels::dimensions((size.w, size.h).into()).ok()?;
    Some(BufferConstraints {
        size,
        shm: vec![wl_shm::Format::Xrgb8888, wl_shm::Format::Argb8888],
        #[cfg(feature = "udev")]
        dma: None,
    })
}

impl App {
    pub(in crate::compositor) fn maintain_remote_capture(&mut self, active: bool) {
        if !active || !self.remote_input_available() {
            self.remote_capture.stop();
            return;
        }
        let outputs = self.capture_output_handles();
        self.remote_capture.sessions.retain(|session| {
            session.alive()
                && source_output(&session.source()).is_some_and(|o| {
                    outputs.contains(&o)
                        && o.current_mode().is_some_and(|mode| {
                            constraints((mode.size.w, mode.size.h).into()).is_some()
                        })
                })
        });
        self.remote_capture.requests.retain(|request| {
            request.session.alive()
                && !request.frame.has_failed()
                && request.deadline > Instant::now()
        });
        for session in &self.remote_capture.sessions {
            let Some(output) = source_output(&session.source()) else {
                continue;
            };
            let Some(mode) = output.current_mode() else {
                continue;
            };
            let size = Size::from((mode.size.w, mode.size.h));
            if session
                .current_constraints()
                .is_some_and(|old| old.size != size)
            {
                if let Some(constraints) = constraints(size) {
                    session.update_constraints(constraints);
                }
                for request in self
                    .remote_capture
                    .requests
                    .extract_if(.., |request| request.session == session.as_ref())
                {
                    request.frame.fail(Failure::BufferConstraints);
                }
            }
        }
        if self
            .remote_capture
            .requests
            .iter()
            .any(|r| r.pixels.is_none())
        {
            self.request_redraw();
        }
    }
}

impl ImageCaptureSourceHandler for App {}

impl OutputCaptureSourceHandler for App {
    fn output_capture_source_state(&mut self) -> &mut OutputCaptureSourceState {
        &mut self.remote_capture.output_source
    }

    fn output_source_created(&mut self, source: ImageCaptureSource, output: &Output) {
        source.user_data().insert_if_missing(|| output.downgrade());
    }
}

impl ImageCopyCaptureHandler for App {
    fn image_copy_capture_state(&mut self) -> &mut ImageCopyCaptureState {
        &mut self.remote_capture.copy
    }

    fn capture_constraints(&mut self, source: &ImageCaptureSource) -> Option<BufferConstraints> {
        if !self.remote_input_available() || self.remote_capture.sessions.len() >= MAX_SESSIONS {
            return None;
        }
        let output = source_output(source)?;
        if !self.capture_output_handles().contains(&output) {
            return None;
        }
        let size = output.current_mode()?.size;
        constraints((size.w, size.h).into())
    }

    fn new_session(&mut self, session: Session) {
        self.remote_capture.sessions.push(session);
    }

    fn frame(&mut self, session: &SessionRef, frame: Frame) {
        if !self.remote_input_available() {
            frame.fail(Failure::Stopped);
            return;
        }
        let Some(output) = source_output(&session.source()) else {
            frame.fail(Failure::Stopped);
            return;
        };
        self.remote_capture.request(session, frame, output.name());
        self.request_redraw();
    }

    fn frame_aborted(&mut self, frame: FrameRef) {
        self.remote_capture
            .requests
            .retain(|request| request.frame != frame);
    }

    fn session_destroyed(&mut self, session: SessionRef) {
        self.remote_capture.sessions.retain(|s| *s != session);
        self.remote_capture
            .requests
            .retain(|r| r.session != session);
    }
}
