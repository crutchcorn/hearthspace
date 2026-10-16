//! A delayed loopback decision endpoint makes nonblocking/cancellation behavior observable.
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use serde_json::Value;

pub(super) struct MockOllama {
    pub(super) url: String,
    requests: mpsc::Receiver<Value>,
    replies: mpsc::Sender<f64>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl MockOllama {
    pub(super) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (requests_tx, requests) = mpsc::channel();
        let (replies, replies_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = thread::spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(stream) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(error) => panic!("mock accept: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut header = Vec::new();
                let mut byte = [0];
                while !header.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                    assert!(header.len() < 8192);
                }
                let header = String::from_utf8(header).unwrap();
                assert!(header.starts_with("POST /v1/systemone "), "{header}");
                let length = header
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                requests_tx
                    .send(serde_json::from_slice(&body).unwrap())
                    .unwrap();
                let probability = replies_rx.recv_timeout(Duration::from_secs(20)).unwrap();
                let body = format!("{{\"answers\":{{\"relevant\":{{\"noul\":{probability}}}}}}}");
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self {
            url,
            requests,
            replies,
            stop,
            thread: Some(thread),
        }
    }

    pub(super) fn request(&self, compositor: &super::HeadlessCompositor) -> Value {
        self.requests
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|error| {
                panic!(
                    "decision request: {error}; group: {}; windows: {}",
                    super::group(compositor),
                    super::query(compositor, "windows")
                )
            })
    }

    pub(super) fn reply(&self, probability: f64) {
        self.replies.send(probability).unwrap();
    }

    pub(super) fn no_request(&self) {
        assert!(
            self.requests
                .recv_timeout(Duration::from_millis(200))
                .is_err()
        );
    }
}

impl Drop for MockOllama {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.replies.send(0.0);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
