use std::{
    cell::Cell,
    io::{self, ErrorKind, Write},
    rc::Rc,
};

use super::ControlReply;

pub(super) const MAX_CONNECTIONS: usize = 32;
pub(super) const MAX_REPLY_BYTES: usize = 64 * 1024 * 1024;
const MAX_TOTAL_REPLY_BYTES: usize = 128 * 1024 * 1024;

#[derive(Default)]
pub(super) struct Limits {
    pub connections: Cell<usize>,
    pub output_bytes: Cell<usize>,
}

/// Header and payload remain separate, so a large PNG is never copied to frame it.
pub(super) struct Output {
    header: Vec<u8>,
    payload: Vec<u8>,
    offset: usize,
    limits: Rc<Limits>,
}

impl Output {
    pub fn new(reply: ControlReply, limits: Rc<Limits>) -> Self {
        let (header, payload) = match reply {
            ControlReply::Ok => (b"ok\n".to_vec(), Vec::new()),
            ControlReply::Payload(payload)
                if payload.len() <= MAX_REPLY_BYTES
                    && limits.output_bytes.get() + payload.len() + 32 <= MAX_TOTAL_REPLY_BYTES =>
            {
                (format!("ok {}\n", payload.len()).into_bytes(), payload)
            }
            ControlReply::Payload(_) => (b"err reply exceeds output budget\n".to_vec(), Vec::new()),
            ControlReply::Err(message) => {
                // Error messages are exactly one bounded line in the legacy wire format.
                let message: String = message
                    .chars()
                    .take(1024)
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect();
                (format!("err {message}\n").into_bytes(), Vec::new())
            }
            ControlReply::Deferred(_) => (b"err nested deferred reply\n".to_vec(), Vec::new()),
        };
        limits
            .output_bytes
            .set(limits.output_bytes.get() + header.len() + payload.len());
        Self {
            header,
            payload,
            offset: 0,
            limits,
        }
    }

    /// Return true only after every byte was sent. A work budget yields to other
    /// compositor sources even when a reader drains its socket continuously.
    pub fn flush(&mut self, mut writer: impl Write) -> io::Result<bool> {
        let mut budget = 256 * 1024;
        while self.offset < self.len() && budget > 0 {
            let remaining = if self.offset < self.header.len() {
                &self.header[self.offset..]
            } else {
                &self.payload[self.offset - self.header.len()..]
            };
            let remaining = &remaining[..remaining.len().min(budget)];
            match writer.write(remaining) {
                Ok(0) => return Err(io::Error::new(ErrorKind::WriteZero, "command peer closed")),
                Ok(written) => {
                    self.offset += written;
                    budget -= written;
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(false),
                Err(error) => return Err(error),
            }
        }
        Ok(self.offset == self.len())
    }

    fn len(&self) -> usize {
        self.header.len() + self.payload.len()
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.limits
            .output_bytes
            .set(self.limits.output_bytes.get() - self.len());
    }
}
