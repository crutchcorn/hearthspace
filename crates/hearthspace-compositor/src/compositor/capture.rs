//! Bounded capture requests. Native pixels are released only on their page flip.
use std::{
    collections::HashMap,
    sync::mpsc,
    time::{Duration, Instant},
};

use super::{CalloopData, ManagedWindowKind, runtime::Backend, shell_integration::ControlReply};
use serde::Serialize;

#[cfg(any(feature = "udev", test))]
mod encoding;
mod frames;
mod legacy;
mod pixels;
#[cfg(test)]
mod tests;

#[cfg(feature = "udev")]
pub(super) use pixels::CursorOverlay;
pub(super) use pixels::{encode_png, read_pixels};

use frames::Frame;

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REQUESTS: usize = 4;

#[derive(Clone, Debug, Serialize)]
pub(super) struct OutputMetadata {
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub scale: i32,
    pub logical_x: i32,
    pub logical_y: i32,
    pub logical_width: i32,
    pub logical_height: i32,
    pub transform: &'static str,
    pub frame_id: u64,
}

struct Request {
    id: u64,
    output: String,
    metadata: bool,
    sender: mpsc::Sender<ControlReply>,
    deadline: Instant,
}

impl Request {
    fn elapsed_ms(&self) -> u128 {
        self.deadline
            .checked_sub(CAPTURE_TIMEOUT)
            .unwrap()
            .elapsed()
            .as_millis()
    }

    fn expire(&self, phase: &'static str, frame_id: u64) -> bool {
        if Instant::now() < self.deadline {
            return true;
        }
        tracing::warn!(capture_id = self.id, output = %self.output, phase, frame_id,
            elapsed_ms = self.elapsed_ms(), "capture request timed out");
        let _ = self
            .sender
            .send(ControlReply::Err("capture presentation timed out".into()));
        false
    }
}

#[derive(Default)]
pub(super) struct CaptureState {
    next_request_id: u64,
    pending: Vec<Request>,
    staged: HashMap<String, Frame>,
    frames: HashMap<String, u64>,
    #[cfg(any(feature = "udev", test))]
    encoder: Option<encoding::Encoder>,
}

impl CaptureState {
    #[cfg(test)]
    pub(in crate::compositor) fn with_test_encoder(
        encode: impl FnMut(
            smithay::utils::Size<i32, smithay::utils::Physical>,
            &[u8],
        ) -> Result<Vec<u8>, String>
        + Send
        + 'static,
    ) -> Self {
        Self {
            encoder: Some(encoding::Encoder::with_encode(encode).unwrap()),
            ..Self::default()
        }
    }

    pub(in crate::compositor) fn request(
        &mut self,
        output: String,
        metadata: bool,
    ) -> ControlReply {
        let count = self.pending.len()
            + self
                .staged
                .values()
                .map(|f| f.requests.len())
                .sum::<usize>();
        if count >= MAX_REQUESTS {
            return ControlReply::Err("too many pending captures".into());
        }
        let (sender, receiver) = mpsc::channel();
        self.next_request_id = self.next_request_id.wrapping_add(1);
        tracing::debug!(capture_id = self.next_request_id, %output,
            frame_id = self.frame_id(&output), pending = count, "capture request accepted");
        self.pending.push(Request {
            id: self.next_request_id,
            output,
            metadata,
            sender,
            deadline: Instant::now() + CAPTURE_TIMEOUT,
        });
        ControlReply::Deferred(receiver)
    }

    pub(super) fn timeout(&self) -> Option<Duration> {
        let deadline = self
            .pending
            .iter()
            .chain(self.staged.values().flat_map(|f| f.requests.iter()))
            .map(|r| r.deadline.saturating_duration_since(Instant::now()))
            .min();
        // CPU encoding never occupies the event-loop thread. Poll completion
        // with a bounded delay, including when a cancelled job owns a slot.
        #[cfg(any(feature = "udev", test))]
        if self.encoder.is_some() {
            return deadline.map(|timeout| timeout.min(Duration::from_millis(10)));
        }
        deadline
    }

    #[cfg(any(feature = "udev", test))]
    pub(super) fn wanted(&self, name: &str) -> bool {
        !self.staged.contains_key(name)
            && self
                .encoder
                .as_ref()
                .is_none_or(encoding::Encoder::available)
            && self.pending.iter().any(|request| request.output == name)
    }

    pub(super) fn frame_id(&self, name: &str) -> u64 {
        self.frames.get(name).copied().unwrap_or(0)
    }

    pub(super) fn fail(&mut self, name: &str, message: &str) {
        self.pending.retain(|request| {
            if request.output != name {
                return true;
            }
            let _ = request.sender.send(ControlReply::Err(message.into()));
            false
        });
        if let Some(frame) = self.staged.remove(name) {
            for request in frame.requests {
                let _ = request.sender.send(ControlReply::Err(message.into()));
            }
        }
    }

    pub(super) fn invalidate(&mut self, message: &str) {
        let names: Vec<_> = self
            .pending
            .iter()
            .map(|r| r.output.clone())
            .chain(self.staged.keys().cloned())
            .collect();
        for name in names {
            self.fail(&name, message);
        }
    }

    pub(in crate::compositor) fn maintain(&mut self, outputs: &[OutputMetadata], active: bool) {
        let names: Vec<_> = self
            .pending
            .iter()
            .map(|r| r.output.clone())
            .chain(self.staged.keys().cloned())
            .collect();
        for name in names {
            if !active {
                self.fail(&name, "native session is inactive");
            } else if !outputs.iter().any(|o| o.name == name) {
                self.fail(&name, "capture output disappeared");
            }
        }
        self.pending.retain(|request| {
            request.expire(
                "awaiting-render",
                self.frames.get(&request.output).copied().unwrap_or(0),
            )
        });
        self.staged.retain(|_, frame| {
            let phase = frame.wait_phase();
            let frame_id = frame.frame_id();
            frame
                .requests
                .retain(|request| request.expire(phase, frame_id));
            !frame.requests.is_empty()
        });
        self.finish_frames();
    }
}

impl CalloopData {
    pub(super) fn maintain_captures(&mut self) {
        if self.state.session_locked() {
            self.captures.invalidate("session is locked");
            return;
        }
        self.captures
            .maintain(&self.state.capture_outputs(), self.capture_active());
        #[cfg(feature = "udev")]
        if self
            .captures
            .pending
            .iter()
            .any(|request| self.captures.wanted(&request.output))
        {
            self.state.request_redraw();
        }
    }

    pub(in crate::compositor) fn capture_active(&self) -> bool {
        match &self.backend {
            #[cfg(feature = "udev")]
            Backend::Udev(backend) => backend.capture_active(),
            _ => true,
        }
    }

    pub(super) fn run_automation_command(&mut self, command: &str) -> Option<ControlReply> {
        let words: Vec<_> = command.split_whitespace().collect();
        match words.as_slice() {
            ["capture"] => Some(self.request_capture(None, true)),
            ["capture", name] => Some(self.request_capture(Some(name), true)),
            ["status"] => Some(ControlReply::Payload(self.automation_status())),
            _ => None,
        }
    }

    pub(super) fn request_capture(&mut self, name: Option<&str>, metadata: bool) -> ControlReply {
        if self.state.session_locked() {
            return ControlReply::Err("session is locked".into());
        }
        if !self.capture_active() {
            return ControlReply::Err("native session is inactive".into());
        }
        let outputs = self.state.capture_outputs();
        let output = match name {
            Some(name) => outputs.iter().find(|o| o.name == name),
            None => outputs.first(),
        };
        let Some(output) = output.cloned() else {
            return ControlReply::Err("unknown capture output".into());
        };
        #[cfg(feature = "udev")]
        if matches!(self.backend, Backend::Udev(_)) {
            let reply = self.captures.request(output.name, metadata);
            self.state.request_redraw();
            return reply;
        }
        // Nested/headless captures retain their existing offscreen contract.
        match self.screenshot_png() {
            Ok(png) => {
                let reply = self.captures.request(output.name.clone(), metadata);
                self.captures.stage(output.clone(), png, "framebuffer");
                self.captures.presented(&output.name, "offscreen");
                reply
            }
            Err(error) => ControlReply::Err(format!("screenshot failed: {error}")),
        }
    }

    fn automation_status(&self) -> Vec<u8> {
        // Presentation alone cannot establish wake readiness: native input
        // recreation must also succeed. This boolean reveals no locked content.
        let recovery_ready = match &self.backend {
            #[cfg(feature = "udev")]
            Backend::Udev(backend) => backend.recovery_complete(),
            _ => true,
        };
        let mut outputs = self.state.capture_outputs();
        for output in &mut outputs {
            output.frame_id = self.captures.frame_id(&output.name);
        }
        let backend = match &self.backend {
            #[cfg(feature = "udev")]
            Backend::Udev(_) => "native",
            #[cfg(feature = "winit")]
            Backend::Winit(_) => "nested",
            Backend::Headless(_) => "headless",
        };
        if self.state.session_locked() {
            return serde_json::to_vec(&serde_json::json!({
                "version": 1, "backend": backend, "active": self.capture_active(),
                "locked": true, "lock_confirmed": self.state.lock_confirmed(),
                "recovery_ready": recovery_ready,
                "windows": [], "outputs": [], "canvas_maps": [], "shell_panels": [], "control_osds": [],
            }))
            .expect("serializable locked status");
        }
        #[cfg(feature = "xwayland")]
        let xwayland_display = self.state.xdisplay.map(|number| format!(":{number}"));
        #[cfg(not(feature = "xwayland"))]
        let xwayland_display: Option<String> = None;
        serde_json::to_vec(&serde_json::json!({
            "version": 1, "backend": backend, "active": self.capture_active(), "outputs": outputs,
            "locked": false, "lock_confirmed": false,
            "recovery_ready": recovery_ready,
            "xwayland_display": xwayland_display,
            "overview": self.state.overview_status(),
            "window_switcher": self.state.window_switcher_status(),
            "window_group": self.state.group_status(),
            "canvas_maps": self.state.canvas_maps_status(),
            "shell_panels": self.state.shell_panels_status(),
            "control_osds": self.state.control_osds_status(),
            "favorite_gesture": self.state.favorite_drawing.status(),
            "viewport": {
                "x": self.state.viewport_offset.x,
                "y": self.state.viewport_offset.y,
                "scale": self.state.viewport_scale,
                "animating": self.state.viewport_animation.is_some(),
                "pinch_active": self.state.pinch_zoom.is_active(),
            },
            "shell_ready": self.state.windows.iter().any(|w| w.kind == ManagedWindowKind::ShellBar && w.content_bbox_size.w > 0),
            "windows": self.state.automation_windows(),
        })).expect("serializable automation status")
    }
}
