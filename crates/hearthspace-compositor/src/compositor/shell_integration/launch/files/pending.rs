//! One bounded worker owns preparation and launch. Calloop owns the deadline and
//! the brief socket-registration handoff; stalled filesystem calls retain a slot.
use crate::compositor::shell_integration::ControlReply;
use hearthspace_shell::desktop_items::file_launch;
use smithay::reexports::calloop::{
    self, EventSource, LoopHandle, Poll, PostAction, Readiness, Token, TokenFactory, channel,
    timer::{TimeoutAction, Timer},
};
use std::{
    io,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const MAX_WORKERS: usize = 4;
static WORKERS: AtomicUsize = AtomicUsize::new(0);
const TIMEOUT: &str = "File launch preparation or application launcher timed out.";

struct Slot;
impl Slot {
    fn acquire() -> Result<Self, String> {
        WORKERS
            .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_WORKERS).then_some(count + 1)
            })
            .map(|_| Self)
            .map_err(|_| "File launcher is busy. Try again shortly.".into())
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

enum Message<P> {
    Prepared(P, mpsc::SyncSender<Command>),
    Finished(Result<(), String>),
}

struct Source<P> {
    events: channel::Channel<Message<P>>,
    timer: Timer,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    reply: Option<mpsc::Sender<ControlReply>>,
}

impl<P> Drop for Source<P> {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

fn reply(sender: &mut Option<mpsc::Sender<ControlReply>>, result: Result<(), String>) {
    if let Some(sender) = sender.take() {
        let _ = sender.send(result.map_or_else(ControlReply::Err, |_| ControlReply::Ok));
    }
}

impl<P> EventSource for Source<P> {
    type Event = P;
    type Metadata = ();
    type Ret = Result<Command, String>;
    type Error = io::Error;

    fn process_events<F>(
        &mut self,
        readiness: Readiness,
        token: Token,
        mut configure: F,
    ) -> io::Result<PostAction>
    where
        F: FnMut(P, &mut ()) -> Self::Ret,
    {
        if Instant::now() >= self.deadline {
            reply(&mut self.reply, Err(TIMEOUT.into()));
            return Ok(PostAction::Remove);
        }
        let deadline = self.deadline;
        let response = &mut self.reply;
        self.events
            .process_events(readiness, token, |event, _| {
                if response.is_none() {
                    return;
                }
                match event {
                    channel::Event::Msg(Message::Prepared(prepared, commands)) => {
                        let result = if Instant::now() >= deadline {
                            Err(TIMEOUT.into())
                        } else {
                            configure(prepared, &mut ())
                        };
                        match result {
                            Ok(command) => {
                                if commands.try_send(command).is_err() {
                                    reply(response, Err("File launch was cancelled.".into()));
                                }
                            }
                            Err(error) => reply(response, Err(error)),
                        }
                    }
                    channel::Event::Msg(Message::Finished(result)) => reply(response, result),
                    channel::Event::Closed => {
                        reply(response, Err("File launch worker stopped.".into()))
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

pub(super) fn start<D: 'static, P: Send + 'static>(
    handle: &LoopHandle<'static, D>,
    budget: Duration,
    prepare: impl FnOnce() -> Result<P, String> + Send + 'static,
    configure: impl FnMut(P, &mut (), &mut D) -> Result<Command, String> + 'static,
) -> ControlReply {
    let slot = match Slot::acquire() {
        Ok(slot) => slot,
        Err(error) => return ControlReply::Err(error),
    };
    let deadline = Instant::now() + budget;
    let cancelled = Arc::new(AtomicBool::new(false));
    let (complete, events) = channel::channel();
    let (send, receive) = mpsc::channel();
    let source = Source {
        events,
        timer: Timer::from_deadline(deadline),
        deadline,
        cancelled: cancelled.clone(),
        reply: Some(send),
    };
    let token = match handle.insert_source(source, configure) {
        Ok(token) => token,
        Err(error) => {
            return ControlReply::Err(format!("Could not register file launcher: {error}"));
        }
    };
    if let Err(error) = std::thread::Builder::new()
        .name("file-launcher".into())
        .spawn(move || {
            let _slot = slot;
            let result = prepare().and_then(|prepared| {
                if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
                    return Err(TIMEOUT.into());
                }
                let (commands, command) = mpsc::sync_channel(1);
                complete
                    .send(Message::Prepared(prepared, commands))
                    .map_err(|_| "File launch was cancelled.".to_string())?;
                let command = command
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .map_err(|_| TIMEOUT.to_string())?;
                if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
                    return Err(TIMEOUT.into());
                }
                file_launch::run_opener_until(command, deadline, &cancelled)
            });
            let _ = complete.send(Message::Finished(result));
        })
    {
        handle.remove(token);
        return ControlReply::Err(format!("Could not start file launcher: {error}"));
    }
    ControlReply::Deferred(receive)
}

#[cfg(test)]
mod tests;
