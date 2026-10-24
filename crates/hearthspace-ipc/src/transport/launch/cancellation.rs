use super::*;
use std::sync::Mutex;

/// One launch's cancellation, including a connected request blocked on its reply.
/// Cancelling never waits for the worker: shutdown wakes it and revokes the peer.
#[derive(Debug, Default)]
pub struct Cancellation {
    pub(super) cancelled: AtomicBool,
    socket: Mutex<Option<UnixStream>>,
}

impl Cancellation {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        let socket = self
            .socket
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(socket) = socket.as_ref() {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }

    pub(super) fn register(&self, stream: &UnixStream) -> io::Result<Registration<'_>> {
        let mut socket = self
            .socket
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // Serialize registration with cancellation. Either cancellation sees
        // this clone, or registration rejects it before any command is written.
        if self.is_cancelled() {
            return Err(self.error());
        }
        if socket.is_some() {
            return Err(io::Error::other("Launch request is already connected."));
        }
        *socket = Some(stream.try_clone()?);
        Ok(Registration(self))
    }

    pub(super) fn error(&self) -> io::Error {
        io::Error::other("Application launch was cancelled.")
    }
}

pub(super) struct Registration<'a>(&'a Cancellation);

impl Drop for Registration<'_> {
    fn drop(&mut self) {
        let mut socket = self
            .0
            .socket
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(socket) = socket.take() {
            // Also release the peer on error/timeout, even before the worker's
            // original descriptor drops. Never retain an extra live connection.
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}
