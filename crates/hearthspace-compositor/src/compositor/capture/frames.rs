use super::{CaptureState, ControlReply, OutputMetadata, Request};
use std::time::Instant;

#[cfg(any(feature = "udev", test))]
use super::encoding::{EncodedPng, Encoder, EncodingResult};
#[cfg(any(feature = "udev", test))]
use std::sync::mpsc;

enum Png {
    Ready(Vec<u8>),
    #[cfg(any(feature = "udev", test))]
    Encoding(mpsc::Receiver<EncodingResult>),
    #[cfg(any(feature = "udev", test))]
    Encoded(EncodedPng),
}

impl Png {
    fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Ready(bytes) => Some(bytes),
            #[cfg(any(feature = "udev", test))]
            Self::Encoded(png) => Some(&png.bytes),
            #[cfg(any(feature = "udev", test))]
            Self::Encoding(_) => None,
        }
    }
}

pub(super) struct Frame {
    pub requests: Vec<Request>,
    output: OutputMetadata,
    png: Png,
    cursor_source: &'static str,
    presentation: Option<&'static str>,
    staged_at: Instant,
}

impl Frame {
    pub(super) fn wait_phase(&self) -> &'static str {
        match (self.png.bytes().is_some(), self.presentation.is_some()) {
            (false, false) => "awaiting-png-and-page-flip",
            (false, true) => "awaiting-png",
            (true, false) => "awaiting-page-flip",
            (true, true) => "reply-ready",
        }
    }

    pub(super) fn frame_id(&self) -> u64 {
        self.output.frame_id
    }
}

#[cfg(test)]
impl Frame {
    pub(super) fn encoding_complete(&self) -> bool {
        self.png.bytes().is_some()
    }
}

impl CaptureState {
    pub(in crate::compositor) fn stage(
        &mut self,
        output: OutputMetadata,
        png: Vec<u8>,
        cursor_source: &'static str,
    ) {
        self.stage_frame(output, Png::Ready(png), cursor_source);
    }

    #[cfg(any(feature = "udev", test))]
    pub(in crate::compositor) fn stage_pixels(
        &mut self,
        output: OutputMetadata,
        size: smithay::utils::Size<i32, smithay::utils::Physical>,
        pixels: Vec<u8>,
        cursor_source: &'static str,
    ) -> Result<(), String> {
        if self.staged.contains_key(&output.name) {
            return Err("capture output already has a submitted frame".into());
        }
        if self.encoder.is_none() {
            self.encoder = Some(Encoder::new()?);
        }
        let span = tracing::debug_span!("capture_encoding", output = %output.name,
            capture_id = self.pending.iter().find(|r| r.output == output.name).map(|r| r.id));
        let png = span.in_scope(|| self.encoder.as_ref().unwrap().submit(size, pixels))?;
        self.stage_frame(output, Png::Encoding(png), cursor_source);
        Ok(())
    }

    fn stage_frame(&mut self, output: OutputMetadata, png: Png, cursor_source: &'static str) {
        let mut requests = Vec::new();
        self.pending.retain_mut(|request| {
            if request.output != output.name {
                return true;
            }
            requests.push(Request {
                id: request.id,
                output: request.output.clone(),
                metadata: request.metadata,
                sender: request.sender.clone(),
                deadline: request.deadline,
            });
            false
        });
        for request in &requests {
            tracing::debug!(capture_id = request.id, output = %output.name,
                frame_id = output.frame_id, elapsed_ms = request.elapsed_ms(),
                "capture pixels staged after submission");
        }
        self.staged.insert(
            output.name.clone(),
            Frame {
                requests,
                output,
                png,
                cursor_source,
                presentation: None,
                staged_at: Instant::now(),
            },
        );
    }

    pub(in crate::compositor) fn presented(&mut self, name: &str, presentation: &'static str) {
        let counter = self.frames.entry(name.to_owned()).or_default();
        *counter += 1;
        if let Some(frame) = self.staged.get_mut(name)
            && frame.presentation.is_none()
        {
            // Encoding can span later frames. Keep the first page flip that
            // belongs to these pixels, even when subsequent flips arrive.
            frame.output.frame_id = *counter;
            frame.presentation = Some(presentation);
            tracing::debug!(
                output = name,
                frame_id = *counter,
                staged_ms = frame.staged_at.elapsed().as_millis(),
                png_ready = frame.png.bytes().is_some(),
                "capture page flip confirmed"
            );
        }
        self.finish_frames();
    }

    pub(super) fn finish_frames(&mut self) {
        self.staged.retain(|_, frame| {
            #[cfg(any(feature = "udev", test))]
            if let Png::Encoding(receiver) = &frame.png {
                match receiver.try_recv() {
                    Ok(Ok(png)) => {
                        tracing::debug!(output = %frame.output.name,
                            frame_id = frame.output.frame_id, bytes = png.bytes.len(),
                            staged_ms = frame.staged_at.elapsed().as_millis(),
                            presented = frame.presentation.is_some(), "capture PNG received");
                        frame.png = Png::Encoded(png);
                    }
                    Err(mpsc::TryRecvError::Empty) => return true,
                    result => {
                        let error = match result {
                            Ok(Err(error)) => error,
                            _ => "capture encoder disconnected".into(),
                        };
                        for request in &frame.requests {
                            let _ = request.sender.send(ControlReply::Err(error.clone()));
                        }
                        return false;
                    }
                }
            }
            let (Some(presentation), Some(png)) = (frame.presentation, frame.png.bytes()) else {
                return true;
            };
            for request in &frame.requests {
                if !request.expire("reply-ready", frame.output.frame_id) {
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
                payload.extend_from_slice(png);
                tracing::debug!(capture_id = request.id, output = %frame.output.name,
                    frame_id = frame.output.frame_id, elapsed_ms = request.elapsed_ms(),
                    "capture reply ready");
                let _ = request.sender.send(ControlReply::Payload(payload));
            }
            false
        });
    }
}
