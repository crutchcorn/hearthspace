//! One bounded JSON line per peer, with independent nonblocking client state.
use std::{
    io::{self, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::model::Shared;

const MAX_CLIENTS: usize = 16;
const MAX_REQUEST: usize = 1024;
const MAX_RESPONSE: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    List {},
    Dismiss { id: u32 },
    Action { id: u32, key: String },
}

#[derive(Default)]
pub(crate) struct Server {
    clients: Vec<Client>,
}

impl Server {
    pub fn poll(&mut self, listener: &UnixListener, state: &Shared) -> io::Result<()> {
        while self.clients.len() < MAX_CLIENTS {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(true)?;
                    self.clients.push(Client {
                        stream,
                        deadline: Instant::now() + Duration::from_millis(250),
                        request: Vec::new(),
                        response: None,
                        written: 0,
                    });
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        self.clients
            .retain_mut(|client| client.poll(state).unwrap_or(false));
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
    fn poll(&mut self, state: &Shared) -> io::Result<bool> {
        if Instant::now() >= self.deadline {
            return Ok(false);
        }
        if self.response.is_none() {
            let mut buffer = [0; MAX_REQUEST];
            match self.stream.read(&mut buffer) {
                Ok(0) => return Ok(false),
                Ok(count) => {
                    let end = buffer[..count].iter().position(|byte| *byte == b'\n');
                    let length = end.map_or(count, |index| index + 1);
                    if self.request.len() + length > MAX_REQUEST {
                        self.respond(json!({"ok": false, "error": "Request too large"}));
                    } else {
                        self.request.extend_from_slice(&buffer[..length]);
                        if end.is_some() {
                            self.respond(handle(&self.request, state));
                        }
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

    fn respond(&mut self, value: Value) {
        let mut response = serde_json::to_vec(&value).unwrap();
        if response.len() >= MAX_RESPONSE {
            response = br#"{"ok":false,"error":"Response too large"}"#.to_vec();
        }
        response.push(b'\n');
        self.response = Some(response);
        self.request.clear();
    }
}

fn handle(bytes: &[u8], state: &Shared) -> Value {
    let request = match serde_json::from_slice(bytes) {
        Ok(request) => request,
        Err(_) => return json!({"ok": false, "error": "Invalid notification command"}),
    };
    let mut state = state.lock().unwrap();
    let result = match request {
        Request::List {} => return json!({"ok": true, "notifications": state.list()}),
        Request::Dismiss { id } => state.close(id, 2, None),
        Request::Action { id, key } => state.invoke(id, &key),
    };
    match result {
        Ok(()) => json!({"ok": true}),
        Err(error) => json!({"ok": false, "error": error}),
    }
}

#[cfg(test)]
mod tests;
