//! One CPU worker, with reservations covering queued, running and finished PNGs.
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
    mpsc::{self, Receiver, SyncSender},
};

use smithay::utils::{Physical, Size};

const MAX_JOBS: usize = 2;
pub(super) type EncodingResult = Result<EncodedPng, String>;

pub(super) struct EncodedPng {
    pub bytes: Vec<u8>,
    // Keep the reservation until the compositor consumes or discards the PNG.
    _permit: Permit,
}

struct Permit(Arc<AtomicUsize>);

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

struct Job {
    size: Size<i32, Physical>,
    pixels: Vec<u8>,
    reply: mpsc::Sender<EncodingResult>,
    permit: Permit,
}

pub(super) struct Encoder {
    sender: SyncSender<Job>,
    jobs: Arc<AtomicUsize>,
}

impl Encoder {
    pub(super) fn new() -> Result<Self, String> {
        Self::with_encode(|size, pixels| {
            super::pixels::encode_png(size, pixels).map_err(|error| error.to_string())
        })
    }

    pub(super) fn with_encode(
        mut encode: impl FnMut(Size<i32, Physical>, &[u8]) -> Result<Vec<u8>, String> + Send + 'static,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel::<Job>(MAX_JOBS);
        std::thread::Builder::new()
            .name("capture-png".into())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    let result = encode(job.size, &job.pixels);
                    drop(job.pixels);
                    let result = result.map(|bytes| EncodedPng {
                        bytes,
                        _permit: job.permit,
                    });
                    let _ = job.reply.send(result);
                }
            })
            .map_err(|error| format!("cannot start capture encoder: {error}"))?;
        Ok(Self {
            sender,
            jobs: Arc::default(),
        })
    }

    pub(super) fn available(&self) -> bool {
        self.jobs.load(Ordering::Acquire) < MAX_JOBS
    }

    pub(super) fn submit(
        &self,
        size: Size<i32, Physical>,
        pixels: Vec<u8>,
    ) -> Result<Receiver<EncodingResult>, String> {
        // Readback uses the same 16 megapixel limit. Validate here as well so
        // every producer obeys the bound: at most two 64 MiB RGBA buffers.
        let (width, height) = super::pixels::dimensions(size).map_err(|e| e.to_string())?;
        if pixels.len() != width * height * 4 {
            return Err("capture pixel length mismatch".into());
        }
        self.jobs
            .try_update(Ordering::AcqRel, Ordering::Acquire, |jobs| {
                (jobs < MAX_JOBS).then_some(jobs + 1)
            })
            .map_err(|_| "capture encoder is busy".to_owned())?;
        let permit = Permit(self.jobs.clone());
        let (reply, receiver) = mpsc::channel();
        self.sender
            .try_send(Job {
                size,
                pixels,
                reply,
                permit,
            })
            .map_err(|_| "capture encoder is unavailable".to_owned())?;
        Ok(receiver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnected_worker_returns_error_and_releases_reservation() {
        let (sender, receiver) = mpsc::sync_channel(MAX_JOBS);
        drop(receiver);
        let encoder = Encoder {
            sender,
            jobs: Arc::default(),
        };
        assert!(encoder.submit((2, 2).into(), vec![0; 16]).is_err());
        assert_eq!(encoder.jobs.load(Ordering::Acquire), 0);
    }
}
