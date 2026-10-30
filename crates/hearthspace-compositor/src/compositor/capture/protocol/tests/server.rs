use super::super::*;
use smithay::{
    output::{Mode, PhysicalProperties, Scale, Subpixel},
    wayland::{
        buffer::BufferHandler,
        compositor::{CompositorClientState, CompositorHandler, CompositorState},
        output::OutputHandler,
        shm::{ShmHandler, ShmState},
    },
};
use std::{os::unix::net::UnixStream, sync::mpsc, thread};
use wayland_server::{Display, protocol::wl_buffer::WlBuffer};

pub(super) struct State {
    pub remote: RemoteCaptureState,
    shm: ShmState,
    compositor: CompositorState,
    _output: Output,
}

impl ImageCaptureSourceHandler for State {}
impl ForeignToplevelListHandler for State {
    fn foreign_toplevel_list_state(
        &mut self,
    ) -> &mut smithay::wayland::foreign_toplevel_list::ForeignToplevelListState {
        &mut self.remote.toplevels.list
    }
}
impl ToplevelCaptureSourceHandler for State {
    fn toplevel_capture_source_state(
        &mut self,
    ) -> &mut smithay::wayland::image_capture_source::ToplevelCaptureSourceState {
        &mut self.remote.toplevels.source
    }
}
impl OutputCaptureSourceHandler for State {
    fn output_capture_source_state(&mut self) -> &mut OutputCaptureSourceState {
        &mut self.remote.output_source
    }
    fn output_source_created(&mut self, source: ImageCaptureSource, output: &Output) {
        source.user_data().insert_if_missing(|| output.downgrade());
    }
}
impl ImageCopyCaptureHandler for State {
    fn image_copy_capture_state(&mut self) -> &mut ImageCopyCaptureState {
        &mut self.remote.copy
    }
    fn capture_constraints(&mut self, _: &ImageCaptureSource) -> Option<BufferConstraints> {
        constraints((2, 2).into())
    }
    fn new_session(&mut self, session: Session) {
        self.remote.sessions.push(session);
    }
    fn frame(&mut self, session: &SessionRef, frame: Frame) {
        self.remote.request(session, frame, "test-output".into());
    }
    fn frame_aborted(&mut self, frame: FrameRef) {
        self.remote.requests.retain(|r| r.frame != frame);
    }
    fn session_destroyed(&mut self, session: SessionRef) {
        self.remote.sessions.retain(|s| *s != session);
        self.remote.requests.retain(|r| r.session != session);
    }
}
impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _: &WlBuffer) {}
}
impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm
    }
}
impl OutputHandler for State {}
impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor
    }
    fn client_compositor_state<'a>(
        &self,
        client: &'a wayland_server::Client,
    ) -> &'a CompositorClientState {
        &client
            .get_data::<crate::compositor::client::ClientState>()
            .unwrap()
            .compositor_state
    }
    fn commit(&mut self, _: &wayland_server::protocol::wl_surface::WlSurface) {}
}
smithay::delegate_dispatch2!(State);

type Action = Box<dyn FnOnce(&mut State) + Send>;
pub(super) struct Server {
    sender: Option<mpsc::Sender<Action>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Server {
    pub fn new(enabled: bool) -> (Self, UnixStream) {
        let (server_socket, client_socket) = UnixStream::pair().unwrap();
        let (sender, receiver) = mpsc::channel::<Action>();
        let thread = thread::spawn(move || {
            let mut display = Display::<State>::new().unwrap();
            let mut handle = display.handle();
            handle
                .insert_client(
                    server_socket,
                    Arc::new(crate::compositor::client::ClientState::default()),
                )
                .unwrap();
            let output = Output::new(
                "test-output".into(),
                PhysicalProperties {
                    size: (1, 1).into(),
                    subpixel: Subpixel::Unknown,
                    make: "test".into(),
                    model: "test".into(),
                    serial_number: "test".into(),
                },
            );
            output.create_global::<State>(&handle);
            output.change_current_state(
                Some(Mode {
                    size: (2, 2).into(),
                    refresh: 60000,
                }),
                Some(Transform::Normal),
                Some(Scale::Integer(1)),
                Some((0, 0).into()),
            );
            let mut state = State {
                remote: RemoteCaptureState::new_for::<State>(&handle, enabled),
                shm: ShmState::new::<State>(&handle, vec![]),
                compositor: CompositorState::new::<State>(&handle),
                _output: output,
            };
            loop {
                display.dispatch_clients(&mut state).unwrap();
                match receiver.try_recv() {
                    Ok(action) => action(&mut state),
                    Err(mpsc::TryRecvError::Disconnected) => break,
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                display.flush_clients().unwrap();
                thread::sleep(Duration::from_millis(1));
            }
        });
        (
            Self {
                sender: Some(sender),
                thread: Some(thread),
            },
            client_socket,
        )
    }

    pub fn run(&self, action: impl FnOnce(&mut State) + Send + 'static) {
        let (sender, receiver) = mpsc::sync_channel(0);
        self.sender
            .as_ref()
            .unwrap()
            .send(Box::new(move |state| {
                action(state);
                sender.send(()).unwrap();
            }))
            .unwrap();
        receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.sender.take();
        self.thread.take().unwrap().join().unwrap();
    }
}
