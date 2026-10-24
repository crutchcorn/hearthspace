//! Bounded nonblocking clients; quiet watches reserve capacity for setters.
use nix::{
    errno::Errno,
    poll::{PollFd, PollFlags, poll},
};
use std::{
    io,
    os::{fd::AsFd, unix::net::UnixListener},
    path::Path,
    time::{Duration, Instant},
};
use tracing::warn;

mod changes;
mod client;
use client::Client;
const MAX_CLIENTS: usize = 32;
const MAX_WATCHERS: usize = 16;
const MAX_UNFINISHED: usize = 8;
const CLIENT_TIMEOUT: Duration = Duration::from_millis(250);

pub(super) fn serve(listener: UnixListener, config_path: &Path) -> io::Result<()> {
    listener.set_nonblocking(true)?;
    let mut server = Server::default();
    let mut changes = changes::Changes::new(config_path)?;
    loop {
        changes.drain(config_path)?;
        server.poll(&listener, config_path)?;
        server.wait_for_work(&listener, &changes)?;
    }
}

#[derive(Default)]
struct Server {
    clients: Vec<Client>,
}

impl Server {
    fn wait_for_work(&self, listener: &UnixListener, changes: &changes::Changes) -> io::Result<()> {
        let mut fds = Vec::with_capacity(self.clients.len() + 2);
        if self.clients.len() < MAX_CLIENTS {
            fds.push(PollFd::new(listener.as_fd(), PollFlags::POLLIN));
        }
        fds.push(PollFd::new(changes.as_fd(), PollFlags::POLLIN));
        let mut timeout = 30_000;
        for client in &self.clients {
            if let Some(interest) = client.interest() {
                fds.push(PollFd::new(client.stream.as_fd(), interest));
            }
            timeout = timeout.min(
                client
                    .deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    + 1,
            );
            if client.retrying() {
                timeout = timeout.min(5);
            }
        }
        match poll(&mut fds, timeout as u16) {
            Ok(_) | Err(Errno::EINTR) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn poll(&mut self, listener: &UnixListener, config_path: &Path) -> io::Result<()> {
        while self.clients.len() < MAX_CLIENTS {
            match listener.accept() {
                Ok((stream, _)) => self.clients.push(Client::new(stream)?),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        let mut watchers = 0;
        let mut unfinished = 0;
        self.clients
            .retain_mut(|client| match client.poll(config_path) {
                Ok(true) => {
                    if client.watching() {
                        watchers += 1;
                        if watchers > MAX_WATCHERS {
                            client.reject("settings watch capacity reached");
                        }
                    } else if client.unfinished() {
                        unfinished += 1;
                        if unfinished > MAX_UNFINISHED {
                            return false;
                        }
                    }
                    true
                }
                Ok(false) => false,
                Err(error) => {
                    warn!(%error, "failed to handle settings client");
                    false
                }
            });
        Ok(())
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod watch_tests;
