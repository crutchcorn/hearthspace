//! A launch is fire-and-forget until a command connection attaches its lifetime.
use std::{
    io,
    os::unix::net::UnixStream,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

pub(in crate::compositor) struct Lifetime {
    cancelled: AtomicBool,
    state: Mutex<State>,
}
struct State {
    deadline: Instant,
    peer: Option<(UnixStream, bool)>,
}

impl Lifetime {
    pub(super) fn new(deadline: Instant) -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            state: Mutex::new(State {
                deadline,
                peer: None,
            }),
        }
    }

    pub(in crate::compositor) fn attach(
        &self,
        stream: &UnixStream,
        deadline: Option<Instant>,
    ) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Launch lifetime unavailable.")?;
        // The caller may shorten preparation, never extend its original budget.
        if let Some(deadline) = deadline {
            state.deadline = state.deadline.min(deadline);
        }
        state.peer = Some((
            stream.try_clone().map_err(|error| error.to_string())?,
            deadline.is_some(),
        ));
        Ok(())
    }

    pub(in crate::compositor) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub(super) fn cancelled(&self) -> &AtomicBool {
        &self.cancelled
    }

    pub(in crate::compositor) fn check(&self) -> Result<Instant, String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("Application launch was cancelled.".into());
        }
        let state = self
            .state
            .lock()
            .map_err(|_| "Launch lifetime unavailable.")?;
        if Instant::now() >= state.deadline {
            self.cancel();
            return Err(super::TIMEOUT.into());
        }
        if let Some((peer, opted_in)) = &state.peer
            && disconnected(peer, *opted_in).unwrap_or(true)
        {
            self.cancel();
            return Err("Application launch was cancelled.".into());
        }
        Ok(state.deadline)
    }
}

fn disconnected(peer: &UnixStream, opted_in: bool) -> io::Result<bool> {
    use smithay::reexports::rustix::event::{PollFd, PollFlags, Timespec, poll};
    // Legacy clients use shutdown(Write) after sending a line. Only the
    // opt-in envelope interprets that half-close (RDHUP) as revocation.
    let interest = if opted_in {
        PollFlags::RDHUP
    } else {
        PollFlags::empty()
    };
    let mut fds = [PollFd::new(peer, interest)];
    poll(&mut fds, Some(&Timespec::default()))?;
    Ok(fds[0]
        .revents()
        .intersects(interest | PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL))
}
