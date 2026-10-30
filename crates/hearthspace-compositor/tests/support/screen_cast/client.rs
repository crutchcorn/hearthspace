//! A real ext-image-copy client; assertions inspect the returned SHM, not screenshots.
use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    os::{
        fd::AsFd,
        unix::{fs::FileExt, net::UnixStream},
    },
    path::Path,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, EventQueue,
    protocol::{wl_buffer, wl_output, wl_shm},
};
use wayland_protocols::ext::{
    foreign_toplevel_list::v1::client::{
        ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
        ext_foreign_toplevel_list_v1::ExtForeignToplevelListV1,
    },
    image_capture_source::v1::client::{
        ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
        ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
    },
    image_copy_capture::v1::client::{
        ext_image_copy_capture_frame_v1::{Event as FrameEvent, ExtImageCopyCaptureFrameV1},
        ext_image_copy_capture_manager_v1::{ExtImageCopyCaptureManagerV1, Options},
        ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
    },
};

#[path = "client/events.rs"]
mod events;

#[derive(Clone)]
pub(super) struct Toplevel {
    pub object: ExtForeignToplevelHandleV1,
    pub identifier: String,
    pub title: String,
    pub app_id: String,
    pub closed: bool,
}

#[derive(Default)]
pub(super) struct SessionState {
    pub size: Option<(i32, i32)>,
    pub stopped: bool,
}

#[derive(Default)]
pub(super) struct State {
    pub globals: Vec<String>,
    pub toplevels: Vec<Toplevel>,
    pub sessions: HashMap<usize, SessionState>,
    pub frames: HashMap<usize, Vec<FrameEvent>>,
    list: Option<ExtForeignToplevelListV1>,
    source: Option<ExtForeignToplevelImageCaptureSourceManagerV1>,
    output_source: Option<ExtOutputImageCaptureSourceManagerV1>,
    manager: Option<ExtImageCopyCaptureManagerV1>,
    output: Option<wl_output::WlOutput>,
    shm: Option<wl_shm::WlShm>,
}

pub(super) struct Client {
    connection: Connection,
    queue: EventQueue<State>,
    pub state: State,
    next_id: usize,
}

pub(super) struct Session {
    pub object: ExtImageCopyCaptureSessionV1,
    pub id: usize,
}

pub(super) struct Capture {
    pub frame: ExtImageCopyCaptureFrameV1,
    pub buffer: wl_buffer::WlBuffer,
    pub id: usize,
    pub size: (i32, i32),
    stride: usize,
    length: usize,
    file: File,
}

impl Capture {
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0; self.length];
        self.file.read_exact_at(&mut bytes, 0).unwrap();
        bytes
    }

    pub fn pixels(&self) -> Vec<u32> {
        let bytes = self.bytes();
        (0..self.size.1 as usize)
            .flat_map(|y| {
                bytes[4 + y * self.stride..4 + y * self.stride + self.size.0 as usize * 4]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| u32::from_ne_bytes(*p))
            })
            .collect()
    }

    pub fn assert_untouched(&self) {
        assert!(self.bytes().iter().all(|byte| *byte == 0x7e));
    }

    pub fn assert_padding(&self) {
        let bytes = self.bytes();
        assert_eq!(&bytes[..4], &[0x7e; 4]);
        for y in 0..self.size.1 as usize {
            let start = 4 + y * self.stride + self.size.0 as usize * 4;
            assert_eq!(&bytes[start..start + 8], &[0x7e; 8]);
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        use wayland_client::Proxy;
        if self.frame.is_alive() {
            self.frame.destroy();
        }
        if self.buffer.is_alive() {
            self.buffer.destroy();
        }
    }
}

impl Client {
    pub fn connect(socket: &Path) -> Self {
        let connection = Connection::from_socket(UnixStream::connect(socket).unwrap()).unwrap();
        Self::from_connection(connection)
    }

    pub fn from_connection(connection: Connection) -> Self {
        let mut queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut state = State::default();
        queue.roundtrip(&mut state).unwrap();
        queue.roundtrip(&mut state).unwrap();
        Self {
            connection,
            queue,
            state,
            next_id: 0,
        }
    }

    pub fn sync(&mut self) {
        self.queue.roundtrip(&mut self.state).unwrap();
    }

    pub fn toplevel(&mut self, app: &str) -> Toplevel {
        self.sync();
        self.state
            .toplevels
            .iter()
            .find(|t| !t.closed && t.app_id == app)
            .unwrap()
            .clone()
    }

    pub fn session(&mut self, toplevel: &Toplevel, cursor: bool) -> Session {
        let source = self.state.source.as_ref().unwrap().create_source(
            &toplevel.object,
            &self.queue.handle(),
            (),
        );
        self.next_id += 1;
        let id = self.next_id;
        self.state.sessions.insert(id, SessionState::default());
        let options = if cursor {
            Options::PaintCursors
        } else {
            Options::empty()
        };
        let object = self.state.manager.as_ref().unwrap().create_session(
            &source,
            options,
            &self.queue.handle(),
            id,
        );
        source.destroy(); // Must not destroy the session's retained source.
        self.sync();
        Session { object, id }
    }

    pub fn output_session(&mut self) -> Session {
        let source = self.state.output_source.as_ref().unwrap().create_source(
            self.state.output.as_ref().unwrap(),
            &self.queue.handle(),
            (),
        );
        self.next_id += 1;
        let id = self.next_id;
        self.state.sessions.insert(id, SessionState::default());
        let object = self.state.manager.as_ref().unwrap().create_session(
            &source,
            Options::empty(),
            &self.queue.handle(),
            id,
        );
        source.destroy();
        self.sync();
        Session { object, id }
    }

    pub fn request(&mut self, session: &Session, size: (i32, i32)) -> Capture {
        let stride = size.0 as usize * 4 + 8;
        let length = 4 + stride * size.1 as usize + 16;
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&vec![0x7e; length]).unwrap();
        let handle = self.queue.handle();
        let pool =
            self.state
                .shm
                .as_ref()
                .unwrap()
                .create_pool(file.as_fd(), length as i32, &handle, ());
        let buffer = pool.create_buffer(
            4,
            size.0,
            size.1,
            stride as i32,
            wl_shm::Format::Xrgb8888,
            &handle,
            (),
        );
        pool.destroy();
        self.next_id += 1;
        let id = self.next_id;
        self.state.frames.insert(id, Vec::new());
        let frame = session.object.create_frame(&handle, id);
        frame.attach_buffer(&buffer);
        frame.damage_buffer(0, 0, size.0, size.1);
        Capture {
            frame,
            buffer,
            id,
            size,
            stride,
            length,
            file,
        }
    }

    pub fn capture(&mut self, session: &Session) -> Capture {
        self.sync();
        let size = self.state.sessions[&session.id].size.unwrap();
        let capture = self.request(session, size);
        capture.frame.capture();
        self.wait_frame(&capture);
        assert!(
            self.state.frames[&capture.id]
                .iter()
                .any(|e| matches!(e, FrameEvent::Ready))
        );
        capture.assert_padding();
        capture
    }

    pub fn wait_frame(&mut self, capture: &Capture) {
        self.connection.flush().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.sync();
            if self.state.frames[&capture.id]
                .iter()
                .any(|e| matches!(e, FrameEvent::Ready | FrameEvent::Failed { .. }))
            {
                return;
            }
            assert!(Instant::now() < deadline, "capture completion timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn assert_failed(&mut self, capture: &Capture) {
        self.wait_frame(capture);
        assert!(
            self.state.frames[&capture.id]
                .iter()
                .any(|e| matches!(e, FrameEvent::Failed { .. }))
        );
        assert!(
            !self.state.frames[&capture.id]
                .iter()
                .any(|e| matches!(e, FrameEvent::Ready))
        );
        capture.assert_untouched();
    }
}
