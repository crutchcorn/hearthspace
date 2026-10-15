use std::{io, os::unix::net::UnixStream, rc::Rc, time::Instant};

use smithay::reexports::calloop::{
    self, EventSource, Interest, Mode, Poll, PostAction, Readiness, Token, TokenFactory,
    generic::Generic,
    timer::{TimeoutAction, Timer},
};
use tracing::debug;

use super::{
    ControlReply,
    connection::{COMMAND_TIMEOUT, Connection},
    reply::{Limits, MAX_CONNECTIONS},
};

/// A socket and timer share one lifecycle. Removing a disconnected or expired
/// client also cancels its timer and releases its connection/output reservations.
pub(super) struct CommandSource {
    socket: Generic<UnixStream>,
    socket_registered: bool,
    timer: Timer,
    connection: Connection,
    limits: Rc<Limits>,
}

impl CommandSource {
    pub fn new(stream: UnixStream, limits: Rc<Limits>) -> Option<Self> {
        if limits.connections.get() >= MAX_CONNECTIONS {
            return None;
        }
        limits.connections.set(limits.connections.get() + 1);
        Some(Self {
            socket: Generic::new(stream, Interest::READ, Mode::Level),
            socket_registered: false,
            timer: Timer::from_duration(COMMAND_TIMEOUT),
            connection: Connection::new(limits.clone(), Instant::now()),
            limits,
        })
    }
}

impl Drop for CommandSource {
    fn drop(&mut self) {
        self.limits
            .connections
            .set(self.limits.connections.get() - 1);
    }
}

impl EventSource for CommandSource {
    type Event = Vec<u8>;
    type Metadata = ();
    type Ret = ControlReply;
    type Error = io::Error;

    fn process_events<F>(
        &mut self,
        readiness: Readiness,
        token: Token,
        mut callback: F,
    ) -> io::Result<PostAction>
    where
        F: FnMut(Self::Event, &mut Self::Metadata) -> Self::Ret,
    {
        let mut socket_event = None;
        self.socket.process_events(readiness, token, |ready, _| {
            socket_event = Some(ready);
            Ok(PostAction::Continue)
        })?;
        let mut timer_event = false;
        self.timer.process_events(readiness, token, |_, _| {
            timer_event = true;
            TimeoutAction::ToDuration(COMMAND_TIMEOUT)
        })?;
        if socket_event.is_none() && !timer_event {
            return Ok(PostAction::Continue);
        }
        let now = Instant::now();
        let result = self.connection.advance(
            self.socket.get_ref(),
            socket_event.is_some_and(|ready| ready.readable),
            now,
            |line| callback(line, &mut ()),
        );
        match result {
            Ok(false) => return Ok(PostAction::Remove),
            Err(error) => {
                debug!(%error, "closing command connection");
                return Ok(PostAction::Remove);
            }
            Ok(true) => {}
        }
        self.socket.interest = self.connection.interest();
        self.timer.set_deadline(self.connection.next_deadline(now));
        Ok(PostAction::Reregister)
    }

    fn register(&mut self, poll: &mut Poll, tokens: &mut TokenFactory) -> calloop::Result<()> {
        self.socket.register(poll, tokens)?;
        self.socket_registered = true;
        self.timer.register(poll, tokens)
    }

    fn reregister(&mut self, poll: &mut Poll, tokens: &mut TokenFactory) -> calloop::Result<()> {
        // A half-closed socket awaiting capture needs only its timer. Unregister
        // it completely to avoid repeated HUP readiness on a fully closed peer.
        let interested = self.socket.interest.readable || self.socket.interest.writable;
        match (self.socket_registered, interested) {
            (true, true) => self.socket.reregister(poll, tokens)?,
            (false, true) => self.socket.register(poll, tokens)?,
            (true, false) => self.socket.unregister(poll)?,
            (false, false) => {}
        }
        self.socket_registered = interested;
        self.timer.reregister(poll, tokens)
    }

    fn unregister(&mut self, poll: &mut Poll) -> calloop::Result<()> {
        if self.socket_registered {
            self.socket.unregister(poll)?;
            self.socket_registered = false;
        }
        self.timer.unregister(poll)
    }
}
