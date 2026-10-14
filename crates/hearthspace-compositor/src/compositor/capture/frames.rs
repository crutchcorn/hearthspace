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
        let png = self.encoder.as_ref().unwrap().submit(size, pixels)?;
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
                presentation: None,
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
        }
        self.finish_frames();
    }

    pub(super) fn finish_frames(&mut self) {
        self.staged.retain(|_, frame| {
            #[cfg(any(feature = "udev", test))]
            if let Png::Encoding(receiver) = &frame.png {
                match receiver.try_recv() {
                    Ok(Ok(png)) => frame.png = Png::Encoded(png),
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
                payload.extend_from_slice(png);
                let _ = request.sender.send(ControlReply::Payload(payload));
            }
            false
        });
    }
}
