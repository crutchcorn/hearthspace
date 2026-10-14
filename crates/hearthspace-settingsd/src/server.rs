//! Bounded nonblocking connections; one unfinished request cannot hold the daemon.
use nix::{
    errno::Errno,
    poll::{PollFd, PollFlags, poll},
};
use std::{
    io::{self, Read, Write},
    os::{
        fd::AsFd,
        unix::net::{UnixListener, UnixStream},
    },
    path::Path,
    time::{Duration, Instant},
};

use hearthspace_ipc::{
    error_response, parse_settings_request,
    transport::{MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES},
};
use tracing::warn;

const MAX_CLIENTS: usize = 32;
const CLIENT_TIMEOUT: Duration = Duration::from_millis(250);

pub(super) fn serve(listener: UnixListener, config_path: &Path) -> io::Result<()> {
    listener.set_nonblocking(true)?;
    let mut server = Server::default();
    loop {
        server.poll(&listener, config_path)?;
        server.wait_for_work(&listener)?;
    }
}

#[derive(Default)]
struct Server {
    clients: Vec<Client>,
}

impl Server {
    fn wait_for_work(&self, listener: &UnixListener) -> io::Result<()> {
        let mut fds = Vec::with_capacity(self.clients.len() + 1);
        if self.clients.len() < MAX_CLIENTS {
            fds.push(PollFd::new(listener.as_fd(), PollFlags::POLLIN));
        }
        let mut timeout = 1000;
        for client in &self.clients {
            let interest = if client.response.is_some() {
                PollFlags::POLLOUT
            } else {
                PollFlags::POLLIN
            };
            fds.push(PollFd::new(client.stream.as_fd(), interest));
            timeout = timeout.min(
                client
                    .deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    + 1,
            );
        }
        match poll(&mut fds, timeout as u16) {
            Ok(_) | Err(Errno::EINTR) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn poll(&mut self, listener: &UnixListener, config_path: &Path) -> io::Result<()> {
        // Never create unbounded worker threads or let slow peers retain more
        // than MAX_CLIENTS * (MAX_REQUEST_BYTES + MAX_RESPONSE_BYTES) bytes.
        while self.clients.len() < MAX_CLIENTS {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(true)?;
                    self.clients.push(Client {
                        stream,
                        deadline: Instant::now() + CLIENT_TIMEOUT,
                        request: Vec::new(),
                        response: None,
                        written: 0,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        self.clients
            .retain_mut(|client| match client.poll(config_path) {
                Ok(keep) => keep,
                Err(error) => {
                    warn!(%error, "failed to handle settings client");
                    false
                }
            });
        Ok(())
    }
}

struct Client {
    stream: UnixStream,
    deadline: Instant,
    request: Vec<u8>,
    response: Option<Vec<u8>>,
    written: usize,
}

impl Client {
    fn poll(&mut self, config_path: &Path) -> io::Result<bool> {
        if Instant::now() >= self.deadline {
            return Ok(false);
        }
        if self.response.is_none() && !self.read_request(config_path)? {
            return Ok(false);
        }
        let Some(response) = &self.response else {
            return Ok(true);
        };
        match self.stream.write(&response[self.written..]) {
            Ok(0) => Ok(false),
            Ok(count) => {
                self.written += count;
                Ok(self.written < response.len())
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(true),
            Err(error) => Err(error),
        }
    }

    fn read_request(&mut self, config_path: &Path) -> io::Result<bool> {
        let mut buffer = [0; 4096];
        loop {
            match self.stream.read(&mut buffer) {
                Ok(0) => return Ok(false), // EOF without a newline is incomplete.
                Ok(count) => {
                    let end = buffer[..count].iter().position(|byte| *byte == b'\n');
                    let length = end.map_or(count, |index| index + 1);
                    if self.request.len() + length > MAX_REQUEST_BYTES {
                        self.respond(error_response("request too large"));
                        return Ok(true);
                    }
                    self.request.extend_from_slice(&buffer[..length]);
                    if end.is_some() {
                        let response = std::str::from_utf8(&self.request)
                            .map_err(|error| error.to_string())
                            .and_then(parse_settings_request)
                            .and_then(|request| {
                                super::handle_request(request, config_path)
                                    .map_err(|error| error.to_string())
                            })
                            .unwrap_or_else(error_response);
                        self.respond(response);
                        return Ok(true);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(true),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => return Ok(true),
                Err(error) => return Err(error),
            }
        }
    }

    fn respond(&mut self, response: String) {
        self.response = Some(if response.len() > MAX_RESPONSE_BYTES {
            error_response("response too large").into_bytes()
        } else {
            response.into_bytes()
        });
        self.request.clear();
    }
}

#[cfg(test)]
mod tests;
