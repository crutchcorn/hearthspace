//! Bounded capture requests. Native pixels are released only on their page flip.
use std::{
    collections::HashMap,
    sync::mpsc,
    time::{Duration, Instant},
};

use super::{CalloopData, ManagedWindowKind, runtime::Backend, shell_integration::ControlReply};
use serde::Serialize;

mod legacy;
mod pixels;
#[cfg(test)]
mod tests;

#[cfg(feature = "udev")]
pub(super) use pixels::CursorOverlay;
pub(super) use pixels::{encode_png, read_pixels};

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
    output: String,
    metadata: bool,
    sender: mpsc::Sender<ControlReply>,
    deadline: Instant,
}

struct Frame {
    requests: Vec<Request>,
    output: OutputMetadata,
    png: Vec<u8>,
    cursor_source: &'static str,
}

#[derive(Default)]
pub(super) struct CaptureState {
    pending: Vec<Request>,
    staged: HashMap<String, Frame>,
    frames: HashMap<String, u64>,
}

impl CaptureState {
    fn request(&mut self, output: String, metadata: bool) -> ControlReply {
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
        self.pending.push(Request {
            output,
            metadata,
            sender,
            deadline: Instant::now() + CAPTURE_TIMEOUT,
        });
        ControlReply::Deferred(receiver)
    }

    pub(super) fn timeout(&self) -> Option<Duration> {
        self.pending
            .iter()
            .chain(self.staged.values().flat_map(|f| f.requests.iter()))
            .map(|r| r.deadline.saturating_duration_since(Instant::now()))
            .min()
    }

    #[cfg(feature = "udev")]
    pub(super) fn wanted(&self, name: &str) -> bool {
        self.pending.iter().any(|request| request.output == name)
    }

    pub(super) fn frame_id(&self, name: &str) -> u64 {
        self.frames.get(name).copied().unwrap_or(0)
    }

    pub(super) fn stage(
        &mut self,
        output: OutputMetadata,
        png: Vec<u8>,
        cursor_source: &'static str,
    ) {
        let mut requests = Vec::new();
        self.pending.retain_mut(|request| {
            if request.output != output.name {
                return true;
            }
            // Moving a sender is unnecessary; cloning preserves receiver lifetime.
            requests.push(Request {
                output: request.output.clone(),
                metadata: request.metadata,
                sender: request.sender.clone(),
                deadline: request.deadline,
            });
            false
        });
        self.staged.insert(
            output.name.clone(),
            Frame {
                requests,
                output,
                png,
                cursor_source,
            },
        );
    }

    pub(super) fn presented(&mut self, name: &str, presentation: &str) {
        let counter = self.frames.entry(name.to_owned()).or_default();
        *counter += 1;
        if let Some(mut frame) = self.staged.remove(name) {
            frame.output.frame_id = *counter;
            for request in frame.requests {
                if Instant::now() >= request.deadline {
                    let _ = request
                        .sender
                        .send(ControlReply::Err("capture presentation timed out".into()));
                    continue;
                }
                let mut payload = Vec::new();
                if request.metadata {
                    let mut metadata =
                        serde_json::to_value(&frame.output).expect("serializable output");
                    metadata["output"] = frame.output.name.clone().into();
                    metadata["version"] = 1.into();
                    metadata["cursor"] = "embedded".into();
                    metadata["cursor_source"] = frame.cursor_source.into();
                    metadata["cursor_sample"] = "render".into();
                    metadata["presentation"] = presentation.into();
                    payload = serde_json::to_vec(&metadata).expect("serializable capture metadata");
                    payload.push(b'\n');
                }
                payload.extend_from_slice(&frame.png);
                let _ = request.sender.send(ControlReply::Payload(payload));
            }
        }
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

    #[cfg(feature = "udev")]
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

    fn maintain(&mut self, outputs: &[OutputMetadata], active: bool) {
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
        let expire = |request: &Request| {
            if Instant::now() < request.deadline {
                return true;
            }
            let _ = request
                .sender
                .send(ControlReply::Err("capture presentation timed out".into()));
            false
        };
        self.pending.retain(expire);
        self.staged.retain(|_, frame| {
            frame.requests.retain(expire);
            !frame.requests.is_empty()
        });
    }
}

impl CalloopData {
    pub(super) fn maintain_captures(&mut self) {
        self.captures
            .maintain(&self.state.capture_outputs(), self.capture_active());
    }

    fn capture_active(&self) -> bool {
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
        #[cfg(feature = "xwayland")]
        let xwayland_display = self.state.xdisplay.map(|number| format!(":{number}"));
        #[cfg(not(feature = "xwayland"))]
        let xwayland_display: Option<String> = None;
        serde_json::to_vec(&serde_json::json!({
            "version": 1, "backend": backend, "active": self.capture_active(), "outputs": outputs,
            "xwayland_display": xwayland_display,
            "viewport": {
                "x": self.state.viewport_offset.x,
                "y": self.state.viewport_offset.y,
                "scale": self.state.viewport_scale,
                "pinch_active": self.state.pinch_zoom.is_active(),
            },
            "shell_ready": self.state.windows.iter().any(|w| w.kind == ManagedWindowKind::ShellBar && w.content_bbox_size.w > 0),
            "windows": self.state.automation_windows(),
        })).expect("serializable automation status")
    }
}
