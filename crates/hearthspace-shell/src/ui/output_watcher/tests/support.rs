//! Minimal compositor peer at the actual Wayland wire boundary. Object IDs are
//! read from client requests, including callbacks which libwayland can recycle.
use std::{
    future::{Future, poll_fn},
    io,
    os::{fd::AsFd, unix::net::UnixStream},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Wake, Waker},
    time::Duration,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
    task::JoinHandle,
};
use wayland_client::Connection;

use super::super::watch_wayland_outputs;

pub(super) async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(3), future)
        .await
        .expect("Wayland test operation stalled")
}

pub(super) async fn counted_wakes<T>(
    future: impl Future<Output = T>,
    wakes: Arc<AtomicUsize>,
) -> T {
    let mut future = std::pin::pin!(future);
    poll_fn(|context| {
        let waker = Waker::from(Arc::new(CountWake {
            parent: context.waker().clone(),
            count: wakes.clone(),
        }));
        future.as_mut().poll(&mut Context::from_waker(&waker))
    })
    .await
}

struct CountWake {
    parent: Waker,
    count: Arc<AtomicUsize>,
}

impl Wake for CountWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.parent.wake_by_ref();
    }
}

pub(super) fn spawn_watcher(
    connection: Connection,
) -> (
    JoinHandle<io::Result<()>>,
    mpsc::UnboundedReceiver<Vec<String>>,
) {
    let (sender, names) = mpsc::unbounded_channel();
    let task = tokio::spawn(watch_wayland_outputs(connection, move |names| {
        sender
            .send(names)
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "names receiver closed"))
    }));
    (task, names)
}

pub(super) struct Peer {
    socket: tokio::net::UnixStream,
    registry: u32,
}

impl Peer {
    pub(super) fn pair() -> (Connection, Self) {
        let (client, server) = UnixStream::pair().unwrap();
        client.set_nonblocking(true).unwrap();
        server.set_nonblocking(true).unwrap();
        (
            Connection::from_socket(client).unwrap(),
            Self {
                socket: tokio::net::UnixStream::from_std(server).unwrap(),
                registry: 0,
            },
        )
    }

    pub(super) fn writer(&self) -> UnixStream {
        self.socket.as_fd().try_clone_to_owned().unwrap().into()
    }

    pub(super) async fn write_bytes(&mut self, bytes: &[u8]) {
        bounded(self.socket.write_all(bytes)).await.unwrap();
    }

    async fn request(&mut self, object: u32, opcode: u16) -> Vec<u8> {
        let mut header = [0; 8];
        bounded(self.socket.read_exact(&mut header)).await.unwrap();
        assert_eq!(word(&header, 0), object);
        let message = word(&header, 4);
        assert_eq!(message as u16, opcode);
        let size = (message >> 16) as usize;
        assert!((8..=4096).contains(&size));
        let mut body = vec![0; size - 8];
        bounded(self.socket.read_exact(&mut body)).await.unwrap();
        body
    }

    async fn event(&mut self, object: u32, opcode: u16, body: &[u8]) {
        bounded(self.socket.write_all(&message(object, opcode, body)))
            .await
            .unwrap();
    }

    pub(super) async fn registry(&mut self) {
        let request = self.request(1, 1).await;
        self.registry = word(&request, 0);
    }

    pub(super) async fn sync(&mut self) -> u32 {
        word(&self.request(1, 0).await, 0)
    }

    pub(super) async fn complete_sync(&mut self, callback: u32) {
        let mut bytes = message(callback, 0, &0_u32.to_ne_bytes());
        bytes.extend(message(1, 1, &callback.to_ne_bytes()));
        bounded(self.socket.write_all(&bytes)).await.unwrap();
    }

    pub(super) async fn advertise(&mut self, global: u32) {
        let mut body = global.to_ne_bytes().to_vec();
        body.extend(string("wl_output"));
        body.extend(4_u32.to_ne_bytes());
        self.event(self.registry, 0, &body).await;
    }

    pub(super) async fn bind(&mut self, global: u32) -> u32 {
        let request = self.request(self.registry, 0).await;
        assert_eq!(word(&request, 0), global);
        assert_eq!(&request[4..20], string("wl_output"));
        assert_eq!(word(&request, 20), 4);
        word(&request, 24)
    }

    pub(super) async fn name(&mut self, output: u32, name: &str) {
        self.write_bytes(&name_message(output, name)).await;
    }

    pub(super) async fn remove(&mut self, global: u32) {
        self.event(self.registry, 1, &global.to_ne_bytes()).await;
    }

    pub(super) async fn bootstrap(&mut self, outputs: &[(u32, &str)]) -> Vec<u32> {
        self.registry().await;
        for &(global, _) in outputs {
            self.advertise(global).await;
        }
        let callback = self.sync().await;
        self.complete_sync(callback).await;
        let mut ids = Vec::new();
        for &(global, name) in outputs {
            let output = self.bind(global).await;
            self.name(output, name).await;
            ids.push(output);
        }
        let callback = self.sync().await;
        self.complete_sync(callback).await;
        ids
    }

    pub(super) async fn expect_eof(&mut self) {
        assert_eq!(bounded(self.socket.read(&mut [0])).await.unwrap(), 0);
    }
}

pub(super) fn name_message(output: u32, name: &str) -> Vec<u8> {
    let mut bytes = message(output, 4, &string(name));
    bytes.extend(message(output, 2, &[]));
    bytes
}

fn message(object: u32, opcode: u16, body: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + body.len());
    bytes.extend(object.to_ne_bytes());
    bytes.extend((((8 + body.len()) as u32) << 16 | u32::from(opcode)).to_ne_bytes());
    bytes.extend(body);
    bytes
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_ne_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn string(value: &str) -> Vec<u8> {
    let length = value.len() + 1;
    let mut bytes = (length as u32).to_ne_bytes().to_vec();
    bytes.extend(value.as_bytes());
    bytes.resize(4 + length.next_multiple_of(4), 0);
    bytes
}
