//! Real wl_buffer.release across worker handoff, GPU reads, and a scanout hold.
use super::{
    OutputKey, SceneStorage,
    retirement::{Retained, Retirement},
};
use crate::compositor::udev::identity::DeviceToken;
use smithay::{
    backend::renderer::{
        sync::{Fence, Interrupted, SyncPoint},
        utils::Buffer,
    },
    wayland::{
        buffer::BufferHandler,
        shm::{ShmHandler, ShmState},
    },
};
use std::{
    num::NonZeroU32,
    os::{
        fd::{AsFd, OwnedFd},
        unix::net::UnixStream,
    },
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop,
    protocol::{wl_buffer, wl_callback, wl_registry, wl_shm, wl_shm_pool},
};
use wayland_server::{Display, protocol::wl_buffer::WlBuffer};

struct Server(ShmState);
impl ShmHandler for Server {
    fn shm_state(&self) -> &ShmState {
        &self.0
    }
}
impl BufferHandler for Server {
    fn buffer_destroyed(&mut self, _: &WlBuffer) {}
}
smithay::delegate_dispatch2!(Server);
#[derive(Default)]
struct Client {
    shm: Option<wl_shm::WlShm>,
    synced: bool,
    releases: usize,
}
impl Dispatch<wl_registry::WlRegistry, ()> for Client {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
            && interface == "wl_shm"
        {
            state.shm = Some(registry.bind(name, 1, q, ()));
        }
    }
}
impl Dispatch<wl_callback::WlCallback, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.synced = true;
    }
}
impl Dispatch<wl_buffer::WlBuffer, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_buffer::Event::Release) {
            state.releases += 1;
        }
    }
}
delegate_noop!(Client: ignore wl_shm::WlShm);
delegate_noop!(Client: ignore wl_shm_pool::WlShmPool);
fn sync(
    connection: &Connection,
    queue: &mut EventQueue<Client>,
    client: &mut Client,
    display: &mut Display<Server>,
    server: &mut Server,
) {
    client.synced = false;
    connection.display().sync(&queue.handle(), ());
    connection.flush().unwrap();
    display.dispatch_clients(server).unwrap();
    display.flush_clients().unwrap();
    for _ in 0..16 {
        queue.dispatch_pending(client).unwrap();
        if client.synced {
            return;
        }
        queue.prepare_read().unwrap().read().unwrap();
    }
    panic!("Wayland sync did not complete");
}
#[derive(Debug)]
struct ReadFence(Arc<AtomicBool>);
impl Fence for ReadFence {
    fn is_signaled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    fn wait(&self) -> Result<(), Interrupted> {
        panic!("must not block on GPU reads")
    }
    fn is_exportable(&self) -> bool {
        false
    }
    fn export(&self) -> Option<OwnedFd> {
        None
    }
}

#[test]
fn client_release_waits_for_worker_reads_and_the_last_scanout_owner()
-> Result<(), Box<dyn std::error::Error>> {
    let mut display = Display::<Server>::new()?;
    let mut dh = display.handle();
    let mut server = Server(ShmState::new::<Server>(&dh, []));
    let (server_socket, client_socket) = UnixStream::pair()?;
    let server_client = dh.insert_client(server_socket, Arc::new(()))?;
    let connection = Connection::from_socket(client_socket).unwrap();
    let mut queue = connection.new_event_queue();
    let mut client = Client::default();
    connection.display().get_registry(&queue.handle(), ());
    sync(
        &connection,
        &mut queue,
        &mut client,
        &mut display,
        &mut server,
    );
    let file = tempfile::tempfile()?;
    file.set_len(16)?;
    let pool = client
        .shm
        .as_ref()
        .unwrap()
        .create_pool(file.as_fd(), 16, &queue.handle(), ());
    let buffer = pool.create_buffer(0, 2, 2, 8, wl_shm::Format::Argb8888, &queue.handle(), ());
    sync(
        &connection,
        &mut queue,
        &mut client,
        &mut display,
        &mut server,
    );
    let resource = server_client
        .object_from_protocol_id::<WlBuffer>(&dh, buffer.id().protocol_id())
        .unwrap();
    let storage = SceneStorage::Client(Buffer::with_implicit(resource));
    // The production element exposes this same guard as UnderlyingStorage::Wayland;
    // Smithay clones it into its pending/current plane state before queueing KMS.
    let (storage, scanout_hold) = std::thread::spawn(move || {
        let SceneStorage::Client(guard) = &storage else {
            unreachable!()
        };
        let held = guard.clone();
        (storage, held)
    })
    .join()
    .unwrap();
    let read_done = Arc::new(AtomicBool::new(false));
    let key = OutputKey {
        device: DeviceToken {
            id: 226,
            incarnation: 1,
        },
        crtc: NonZeroU32::new(7).unwrap().into(),
        generation: 1,
    };
    let mut retirement = Retirement::default();
    retirement.retain(
        Retained {
            owner: storage,
            key,
            sequence: 1,
            submitted: true,
            producer: SyncPoint::signaled(),
            reader: Some(SyncPoint::from(ReadFence(read_done.clone()))),
            uncertain: false,
        },
        |_| panic!("submitted frame discarded"),
    );
    retirement.poll(|_| panic!("released while GPU was reading"));
    sync(
        &connection,
        &mut queue,
        &mut client,
        &mut display,
        &mut server,
    );
    assert_eq!(client.releases, 0);
    read_done.store(true, Ordering::Release);
    retirement.poll(|_| {});
    assert!(retirement.is_empty());
    sync(
        &connection,
        &mut queue,
        &mut client,
        &mut display,
        &mut server,
    );
    assert_eq!(
        client.releases, 0,
        "scene retirement released a scanned-out buffer"
    );
    drop(scanout_hold);
    sync(
        &connection,
        &mut queue,
        &mut client,
        &mut display,
        &mut server,
    );
    assert_eq!(client.releases, 1);
    Ok(())
}
