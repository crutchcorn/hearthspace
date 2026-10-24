//! One bounded worker owns preparation and launch. Calloop owns the deadline and
//! the brief socket-registration handoff; stalled filesystem calls retain a slot.
use crate::compositor::shell_integration::ControlReply;
use hearthspace_desktop::file_launch;
use smithay::reexports::calloop::{
    self, EventSource, LoopHandle, Poll, PostAction, Readiness, Token, TokenFactory, channel,
    timer::{TimeoutAction, Timer},
};
use std::{
    io,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

mod caller;
pub(in crate::compositor) use caller::Lifetime;

const MAX_WORKERS: usize = 4;
static WORKERS: AtomicUsize = AtomicUsize::new(0);
const TIMEOUT: &str = "Application launch preparation or application launcher timed out.";

struct Slot;
impl Slot {
    fn acquire() -> Result<Self, String> {
        WORKERS
            .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_WORKERS).then_some(count + 1)
            })
            .map(|_| Self)
            .map_err(|_| "Application launcher is busy. Try again shortly.".into())
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) struct Execution {
    pub command: Command,
    pub display: Option<hearthspace_desktop::activation::DisplayUpdate>,
    detached: bool,
}

impl Execution {
    pub fn helper(command: Command) -> Self {
        Self {
            command,
            display: None,
            detached: false,
        }
    }
    pub fn detached(command: Command) -> Self {
        Self {
            command,
            display: None,
            detached: true,
        }
    }
}

pub(super) enum Event<P> {
    Prepared(P),
    Ready(Option<hearthspace_desktop::activation::DisplayUpdate>),
    Finished(Result<(), String>),
}

enum Message<P> {
    Prepared(P, mpsc::SyncSender<Execution>),
    Ready(
        Option<hearthspace_desktop::activation::DisplayUpdate>,
        mpsc::SyncSender<()>,
    ),
    Finished(Result<(), String>),
}

struct Source<P> {
    events: channel::Channel<Message<P>>,
    timer: Timer,
    lifetime: Arc<Lifetime>,
    reply: Option<mpsc::Sender<ControlReply>>,
}

impl<P> Drop for Source<P> {
    fn drop(&mut self) {
        self.lifetime.cancel();
    }
}

fn reply(sender: &mut Option<mpsc::Sender<ControlReply>>, result: Result<(), String>) {
    if let Err(error) = &result {
        tracing::warn!(%error, "application launch failed");
    }
    if let Some(sender) = sender.take() {
        let _ = sender.send(result.map_or_else(ControlReply::Err, |_| ControlReply::Ok));
    }
}

impl<P> EventSource for Source<P> {
    type Event = Event<P>;
    type Metadata = ();
    type Ret = Result<Option<Execution>, String>;
    type Error = io::Error;

    fn process_events<F>(
        &mut self,
        readiness: Readiness,
        token: Token,
        mut configure: F,
    ) -> io::Result<PostAction>
    where
        F: FnMut(Event<P>, &mut ()) -> Self::Ret,
    {
        if let Err(error) = self.lifetime.check() {
            reply(&mut self.reply, Err(error));
            return Ok(PostAction::Remove);
        }
        let lifetime = &self.lifetime;
        let response = &mut self.reply;
        self.events
            .process_events(readiness, token, |event, _| {
                if response.is_none() {
                    return;
                }
                match event {
                    channel::Event::Msg(Message::Prepared(prepared, commands)) => {
                        let result = lifetime
                            .check()
                            .and_then(|_| configure(Event::Prepared(prepared), &mut ()));
                        match result {
                            Ok(Some(command)) => {
                                if commands.try_send(command).is_err() {
                                    reply(
                                        response,
                                        Err("Application launch was cancelled.".into()),
                                    );
                                }
                            }
                            Ok(None) => reply(response, Err("Missing prepared launch".into())),
                            Err(error) => reply(response, Err(error)),
                        }
                    }
                    channel::Event::Msg(Message::Ready(display, accepted)) => {
                        match lifetime
                            .check()
                            .and_then(|_| configure(Event::Ready(display), &mut ()))
                        {
                            Ok(_) => {
                                if accepted.try_send(()).is_err() {
                                    reply(
                                        response,
                                        Err("Application launch was cancelled.".into()),
                                    );
                                }
                            }
                            Err(error) => reply(response, Err(error)),
                        }
                    }
                    channel::Event::Msg(Message::Finished(result)) => {
                        let result = configure(Event::Finished(result), &mut ()).map(|_| ());
                        reply(response, result);
                    }
                    channel::Event::Closed => {
                        reply(response, Err("Application launch worker stopped.".into()))
                    }
                }
            })
            .map_err(io::Error::other)?;
        self.timer
            .process_events(readiness, token, |_, _| {
                reply(response, Err(TIMEOUT.into()));
                TimeoutAction::Drop
            })
            .map_err(io::Error::other)?;
        Ok(if response.is_none() {
            PostAction::Remove
        } else {
            PostAction::Continue
        })
    }

    fn register(&mut self, poll: &mut Poll, tokens: &mut TokenFactory) -> calloop::Result<()> {
        self.events.register(poll, tokens)?;
        self.timer.register(poll, tokens)
    }
    fn reregister(&mut self, poll: &mut Poll, tokens: &mut TokenFactory) -> calloop::Result<()> {
        self.events.reregister(poll, tokens)?;
        self.timer.reregister(poll, tokens)
    }
    fn unregister(&mut self, poll: &mut Poll) -> calloop::Result<()> {
        self.events.unregister(poll)?;
        self.timer.unregister(poll)
    }
}

#[cfg(test)]
pub(super) fn start<D: 'static, P: Send + 'static>(
    handle: &LoopHandle<'static, D>,
    budget: Duration,
    prepare: impl FnOnce() -> Result<P, String> + Send + 'static,
    configure: impl FnMut(P, &mut (), &mut D) -> Result<Execution, String> + 'static,
) -> ControlReply {
    start_checked(
        handle,
        budget,
        prepare,
        configure,
        |_, _| Ok(()),
        |result, _| result,
    )
}

pub(super) fn start_checked<D: 'static, P: Send + 'static>(
    handle: &LoopHandle<'static, D>,
    budget: Duration,
    prepare: impl FnOnce() -> Result<P, String> + Send + 'static,
    mut configure: impl FnMut(P, &mut (), &mut D) -> Result<Execution, String> + 'static,
    mut admit: impl FnMut(
        Option<hearthspace_desktop::activation::DisplayUpdate>,
        &mut D,
    ) -> Result<(), String>
    + 'static,
    mut finish: impl FnMut(Result<(), String>, &mut D) -> Result<(), String> + 'static,
) -> ControlReply {
    let slot = match Slot::acquire() {
        Ok(slot) => slot,
        Err(error) => return ControlReply::Err(error),
    };
    let deadline = Instant::now() + budget;
    let lifetime = Arc::new(Lifetime::new(deadline));
    let (complete, events) = channel::channel();
    let (send, receive) = mpsc::channel();
    let source = Source {
        events,
        timer: Timer::from_deadline(deadline),
        lifetime: lifetime.clone(),
        reply: Some(send),
    };
    let token = match handle.insert_source(source, move |event, metadata, data| match event {
        Event::Prepared(prepared) => configure(prepared, metadata, data).map(Some),
        Event::Ready(display) => admit(display, data).map(|()| None),
        Event::Finished(result) => finish(result, data).map(|()| None),
    }) {
        Ok(token) => token,
        Err(error) => {
            return ControlReply::Err(format!("Could not register application launcher: {error}"));
        }
    };
    let worker_lifetime = lifetime.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("app-launcher".into())
        .spawn(move || {
            let _slot = slot;
            let result = prepare().and_then(|prepared| {
                let deadline = worker_lifetime.check()?;
                let (commands, command) = mpsc::sync_channel(1);
                complete
                    .send(Message::Prepared(prepared, commands))
                    .map_err(|_| "Application launch was cancelled.".to_string())?;
                let command = command
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .map_err(|_| TIMEOUT.to_string())?;
                let deadline = worker_lifetime.check()?;
                if let Some(display) = &command.display {
                    display
                        .synchronize(deadline, worker_lifetime.cancelled())
                        .map_err(|error| {
                            format!("Could not prepare application display: {error}")
                        })?;
                }
                let (accept, accepted) = mpsc::sync_channel(1);
                complete
                    .send(Message::Ready(command.display, accept))
                    .map_err(|_| "Application launch was cancelled.".to_string())?;
                accepted
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .map_err(|_| "Application launch was cancelled.".to_string())?;
                // Read the peer directly, independent of event-source ordering:
                // a disconnect during preparation must not create a late child.
                let deadline = worker_lifetime.check()?;
                if command.detached {
                    file_launch::spawn_until(command.command, deadline, worker_lifetime.cancelled())
                } else {
                    file_launch::run_opener_until(
                        command.command,
                        deadline,
                        worker_lifetime.cancelled(),
                    )
                }
            });
            let _ = complete.send(Message::Finished(result));
        })
    {
        handle.remove(token);
        return ControlReply::Err(format!("Could not start application launcher: {error}"));
    }
    ControlReply::Launch(receive, lifetime)
}

#[cfg(test)]
mod tests;
