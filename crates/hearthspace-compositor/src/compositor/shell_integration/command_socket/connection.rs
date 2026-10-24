use std::{
    io::{self, ErrorKind, Read},
    os::unix::net::UnixStream,
    rc::Rc,
    sync::{
        Arc,
        mpsc::{Receiver, TryRecvError},
    },
    time::{Duration, Instant},
};

use smithay::reexports::calloop::Interest;

use super::super::launch::pending::Lifetime;
use hearthspace_ipc::transport::launch::Envelope;

use super::{
    ControlReply,
    reply::{Limits, Output},
};

pub(super) const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const DEFERRED_POLL: Duration = Duration::from_millis(20);
// A 16 KiB UTF-8 clipboard is hex encoded on the single-line wire protocol.
pub(super) const MAX_COMMAND_BYTES: usize = 33 * 1024;
const MAX_INPUT_BYTES: usize = 64 * 1024;

pub(super) struct Connection {
    input: Vec<u8>,
    output: Option<Output>,
    pending: Option<Receiver<ControlReply>>,
    launch: Option<Arc<Lifetime>>,
    eof: bool,
    close_after_reply: bool,
    deadline: Instant,
    limits: Rc<Limits>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(launch) = &self.launch {
            launch.cancel();
        }
    }
}

impl Connection {
    pub fn new(limits: Rc<Limits>, now: Instant) -> Self {
        Self {
            input: Vec::new(),
            output: None,
            pending: None,
            launch: None,
            eof: false,
            close_after_reply: false,
            deadline: now + COMMAND_TIMEOUT,
            limits,
        }
    }

    pub fn interest(&self) -> Interest {
        Interest {
            readable: !self.eof && !self.close_after_reply,
            writable: self.output.is_some(),
        }
    }

    pub fn next_deadline(&self, now: Instant) -> Instant {
        if self.pending.is_some() {
            self.deadline.min(now + DEFERRED_POLL)
        } else if self.output.is_none() && self.has_command() {
            // More than the per-dispatch work limit remains in the input buffer.
            now + Duration::from_millis(1)
        } else {
            self.deadline
        }
    }

    /// Returning false removes the source and drops all pending bytes/receivers.
    pub fn advance(
        &mut self,
        stream: &UnixStream,
        readable: bool,
        now: Instant,
        mut run: impl FnMut(Vec<u8>) -> ControlReply,
    ) -> io::Result<bool> {
        // A busy compositor can delay dispatch after the producer has already
        // completed. Preserve that result (including its precise capture error)
        // before applying the transport's fallback deadline.
        if let Some(pending) = &self.pending {
            let reply = match pending.try_recv() {
                Ok(reply) => Some(reply),
                Err(TryRecvError::Disconnected) => {
                    Some(ControlReply::Err("command cancelled".into()))
                }
                Err(TryRecvError::Empty) => None,
            };
            if let Some(reply) = reply {
                self.pending = None;
                self.launch = None;
                // Deferred chains are not allowed to extend a command's deadline.
                self.output = Some(Output::new(reply, self.limits.clone()));
                self.deadline = now + COMMAND_TIMEOUT;
            }
        }
        if let Some(launch) = &self.launch
            && let Err(error) = launch.check()
        {
            self.pending = None;
            self.launch = None;
            self.close_after_reply = true;
            self.set_reply(ControlReply::Err(error), now);
        }
        if now >= self.deadline {
            if self.pending.take().is_some() {
                if let Some(launch) = self.launch.take() {
                    launch.cancel();
                }
                self.close_after_reply = true;
                self.set_reply(ControlReply::Err("command timed out".into()), now);
            } else {
                return Ok(false);
            }
        }
        if readable && self.interest().readable {
            self.read_available(stream)?;
        }
        for _ in 0..16 {
            if let Some(output) = self.output.as_mut() {
                if !output.flush(stream)? {
                    return Ok(true);
                }
                self.output = None;
                self.deadline = now + COMMAND_TIMEOUT;
            }
            if self.close_after_reply {
                return Ok(false);
            }
            if self.pending.is_some() {
                return Ok(true);
            }
            let line_length = self
                .input
                .iter()
                .position(|&byte| byte == b'\n')
                .map(|position| position + 1)
                .or_else(|| (self.eof && !self.input.is_empty()).then_some(self.input.len()));
            if line_length.unwrap_or(self.input.len()) > MAX_COMMAND_BYTES {
                self.close_after_reply = true;
                self.set_reply(ControlReply::Err("command exceeds input limit".into()), now);
                continue;
            }
            let Some(length) = line_length else {
                return Ok(!self.eof);
            };
            let line: Vec<u8> = self.input.drain(..length).collect();
            let (line, deadline) = match Envelope::decode(&line) {
                Ok(Some(envelope)) => (
                    format!("{}\n", envelope.command).into_bytes(),
                    Some(envelope.deadline),
                ),
                Ok(None) => (line, None),
                Err(error) => {
                    self.close_after_reply = true;
                    self.set_reply(ControlReply::Err(error.to_string()), now);
                    continue;
                }
            };
            let reply = run(line);
            if let ControlReply::Launch(_, lifetime) = &reply {
                if let Err(error) = lifetime.attach(stream, deadline) {
                    lifetime.cancel();
                    self.set_reply(ControlReply::Err(error), now);
                    continue;
                }
                // Keep the transport bounded by the same deadline as preparation.
                self.set_reply(reply, now);
                if let Some(deadline) = deadline {
                    self.deadline = self.deadline.min(deadline);
                }
            } else {
                self.set_reply(reply, now);
            }
        }
        Ok(true)
    }

    fn set_reply(&mut self, reply: ControlReply, now: Instant) {
        self.deadline = now + COMMAND_TIMEOUT;
        match reply {
            ControlReply::Deferred(receiver) => self.pending = Some(receiver),
            ControlReply::Launch(receiver, lifetime) => {
                self.pending = Some(receiver);
                self.launch = Some(lifetime);
            }
            reply => self.output = Some(Output::new(reply, self.limits.clone())),
        }
    }

    fn has_command(&self) -> bool {
        self.input.contains(&b'\n') || (self.eof && !self.input.is_empty())
    }

    fn read_available(&mut self, mut stream: &UnixStream) -> io::Result<()> {
        let mut chunk = [0; 1024];
        for _ in 0..64 {
            match stream.read(&mut chunk) {
                Ok(0) => {
                    self.eof = true;
                    break;
                }
                Ok(length) => {
                    if self.input.len() + length > MAX_INPUT_BYTES {
                        return Err(io::Error::new(
                            ErrorKind::InvalidData,
                            "command input queue full",
                        ));
                    }
                    self.input.extend_from_slice(&chunk[..length]);
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
