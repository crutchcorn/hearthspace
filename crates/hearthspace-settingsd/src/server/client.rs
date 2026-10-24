use hearthspace_ipc::{
    SettingsRequest, error_response, parse_settings_request,
    settings_snapshot::{WatchRequest, snapshot_response},
    transport::{MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES},
};
use nix::poll::PollFlags;
use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::{Duration, Instant},
};

pub(super) struct Client {
    pub(super) stream: UnixStream,
    pub(super) deadline: Instant,
    pub(super) request: Vec<u8>,
    response: Option<Vec<u8>>,
    written: usize,
    watch: Option<WatchRequest>,
    pending: Option<SettingsRequest>,
    committed_response: bool,
}

impl Client {
    pub(super) fn new(stream: UnixStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            deadline: Instant::now() + super::CLIENT_TIMEOUT,
            request: Vec::new(),
            response: None,
            written: 0,
            watch: None,
            pending: None,
            committed_response: false,
        })
    }
    pub(super) fn watching(&self) -> bool {
        self.watch.is_some()
    }
    pub(super) fn retrying(&self) -> bool {
        self.pending.is_some()
    }
    pub(super) fn unfinished(&self) -> bool {
        self.response.is_none() && self.watch.is_none() && self.pending.is_none()
    }
    pub(super) fn interest(&self) -> Option<PollFlags> {
        if self.response.is_some() {
            Some(PollFlags::POLLOUT)
        } else if self.unfinished() {
            Some(PollFlags::POLLIN)
        } else {
            None
        } // Clients normally half-close requests; do not spin on EOF.
    }
    pub(super) fn reject(&mut self, message: &str) {
        self.respond(error_response(message));
    }

    pub(super) fn poll(&mut self, config_path: &Path) -> io::Result<bool> {
        if self.watch.is_none() && Instant::now() >= self.deadline {
            if self.pending.is_some() {
                self.reject("settings writer lock timed out");
            } else {
                return Ok(false);
            }
        }
        if self.unfinished() && !self.read_request(config_path)? {
            return Ok(false);
        }
        if let Some(request) = &self.pending {
            let result = if self.committed_response {
                super::super::storage::update(config_path, |settings| {
                    super::super::mutation::apply(settings, request.clone())
                })
                .and_then(|snapshot| snapshot_response(Some(snapshot)))
            } else {
                super::super::handle_request(request.clone(), config_path)
            };
            match result {
                Ok(response) => self.respond(response),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => self.respond(error_response(error)),
            }
        }
        if let Some(watch) = &self.watch {
            match super::super::storage::load_snapshot(config_path) {
                Ok(snapshot) if snapshot.cursor() != watch.after => {
                    self.respond(snapshot_response(Some(snapshot))?)
                }
                Ok(_) if Instant::now() >= self.deadline => self.respond(snapshot_response(None)?),
                Ok(_) => {}
                Err(error) => self.respond(error_response(error)),
            }
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
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(true)
            }
            Err(error) => Err(error),
        }
    }

    fn read_request(&mut self, config_path: &Path) -> io::Result<bool> {
        let mut buffer = [0; 4096];
        loop {
            match self.stream.read(&mut buffer) {
                Ok(0) => return Ok(false),
                Ok(count) => {
                    let end = buffer[..count].iter().position(|byte| *byte == b'\n');
                    let length = end.map_or(count, |index| index + 1);
                    if self.request.len() + length > MAX_REQUEST_BYTES {
                        self.reject("request too large");
                        return Ok(true);
                    }
                    self.request.extend_from_slice(&buffer[..length]);
                    if end.is_some() {
                        let request = String::from_utf8(std::mem::take(&mut self.request));
                        let result = request
                            .map_err(|error| error.to_string())
                            .and_then(|line| self.dispatch(&line, config_path));
                        if let Err(error) = result {
                            self.respond(error_response(error));
                        }
                        return Ok(true);
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    return Ok(true);
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn dispatch(&mut self, line: &str, config_path: &Path) -> Result<(), String> {
        if line.trim() == "get snapshot" {
            let snapshot = super::super::storage::load_snapshot(config_path)
                .map_err(|error| error.to_string())?;
            self.respond(snapshot_response(Some(snapshot)).map_err(|error| error.to_string())?);
        } else if let Some(payload) = line.trim().strip_prefix("watch settings ") {
            let watch = WatchRequest::parse(payload)?;
            self.deadline = Instant::now() + Duration::from_millis(watch.wait_ms);
            self.watch = Some(watch);
        } else {
            let line = if let Some(line) = line.strip_prefix("commit ") {
                self.committed_response = true;
                line
            } else {
                line
            };
            self.pending = Some(parse_settings_request(line)?);
            self.deadline = Instant::now() + Duration::from_millis(500);
        }
        Ok(())
    }

    fn respond(&mut self, response: String) {
        self.response = Some(if response.len() > MAX_RESPONSE_BYTES {
            error_response("response too large").into_bytes()
        } else {
            response.into_bytes()
        });
        self.request.clear();
        self.pending = None;
        self.watch = None;
        self.deadline = Instant::now() + super::CLIENT_TIMEOUT;
    }
}
