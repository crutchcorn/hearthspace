use std::{
    fs::File,
    io::Write,
    os::{
        fd::AsFd,
        unix::{fs::FileExt, net::UnixStream},
    },
};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool},
};
use wayland_protocols::ext::{
    image_capture_source::v1::client::{
        ext_image_capture_source_v1, ext_output_image_capture_source_manager_v1,
    },
    image_copy_capture::v1::client::{
        ext_image_copy_capture_frame_v1 as frame, ext_image_copy_capture_manager_v1 as manager,
        ext_image_copy_capture_session_v1 as session,
    },
};

#[derive(Default)]
pub(super) struct Events {
    source:
        Option<ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1>,
    pub manager: Option<manager::ExtImageCopyCaptureManagerV1>,
    output: Option<wl_output::WlOutput>,
    shm: Option<wl_shm::WlShm>,
    pub frames: Vec<frame::Event>,
    pub stopped: usize,
}

pub(super) struct Client {
    _connection: Connection,
    queue: EventQueue<Events>,
    pub events: Events,
}

pub(super) struct Buffer {
    pub object: wl_buffer::WlBuffer,
    file: File,
}

impl Buffer {
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0; 40];
        self.file.read_exact_at(&mut bytes, 0).unwrap();
        bytes
    }
}

impl Client {
    pub fn new(socket: UnixStream) -> Self {
        let connection = Connection::from_socket(socket).unwrap();
        let mut queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut events = Events::default();
        queue.roundtrip(&mut events).unwrap();
        Self {
            _connection: connection,
            queue,
            events,
        }
    }

    pub fn sync(&mut self) {
        self.queue.roundtrip(&mut self.events).unwrap();
    }

    pub fn session(&mut self, cursor: bool) -> session::ExtImageCopyCaptureSessionV1 {
        let handle = self.queue.handle();
        let source = self.events.source.as_ref().unwrap().create_source(
            self.events.output.as_ref().unwrap(),
            &handle,
            (),
        );
        let flags = if cursor {
            manager::Options::PaintCursors
        } else {
            manager::Options::empty()
        };
        let session =
            self.events
                .manager
                .as_ref()
                .unwrap()
                .create_session(&source, flags, &handle, ());
        // The capture source protocol explicitly permits this: sessions retain
        // their source when the client destroys the creating source object.
        source.destroy();
        self.sync();
        session
    }

    pub fn capture(
        &mut self,
        session: &session::ExtImageCopyCaptureSessionV1,
        width: i32,
    ) -> (frame::ExtImageCopyCaptureFrameV1, Buffer) {
        let handle = self.queue.handle();
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&[0x7e; 40]).unwrap();
        let pool = self
            .events
            .shm
            .as_ref()
            .unwrap()
            .create_pool(file.as_fd(), 40, &handle, ());
        let buffer = pool.create_buffer(4, width, 2, 12, wl_shm::Format::Xrgb8888, &handle, ());
        pool.destroy();
        let frame = session.create_frame(&handle, ());
        frame.attach_buffer(&buffer);
        frame.damage_buffer(0, 0, width, 2);
        frame.capture();
        self.sync();
        (
            frame,
            Buffer {
                object: buffer,
                file,
            },
        )
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Events {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_shm" => state.shm = Some(registry.bind(name, version.min(1), handle, ())),
            "wl_output" => state.output = Some(registry.bind(name, version.min(4), handle, ())),
            "ext_output_image_capture_source_manager_v1" => {
                state.source = Some(registry.bind(name, 1, handle, ()))
            }
            "ext_image_copy_capture_manager_v1" => {
                state.manager = Some(registry.bind(name, 1, handle, ()))
            }
            _ => {}
        }
    }
}

impl Dispatch<frame::ExtImageCopyCaptureFrameV1, ()> for Events {
    fn event(
        state: &mut Self,
        _: &frame::ExtImageCopyCaptureFrameV1,
        event: frame::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.frames.push(event);
    }
}
impl Dispatch<session::ExtImageCopyCaptureSessionV1, ()> for Events {
    fn event(
        state: &mut Self,
        _: &session::ExtImageCopyCaptureSessionV1,
        event: session::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, session::Event::Stopped) {
            state.stopped += 1;
        }
    }
}
delegate_noop!(Events: ignore wl_output::WlOutput);
delegate_noop!(Events: ignore wl_shm::WlShm);
delegate_noop!(Events: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Events: ignore wl_buffer::WlBuffer);
delegate_noop!(Events: ignore ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1);
delegate_noop!(Events: ignore ext_image_capture_source_v1::ExtImageCaptureSourceV1);
delegate_noop!(Events: ignore manager::ExtImageCopyCaptureManagerV1);
