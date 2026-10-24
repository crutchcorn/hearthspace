//! Drive the real task view and its completion callback, without native UI.
use super::ShellState;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use xilem::{
    ViewCtx,
    core::{
        MessageCtx, MessageResult, NoElement, ProxyError, RawProxy, SendMessage, View, ViewId,
        ViewPathTracker,
    },
};

type Message = (Arc<[ViewId]>, SendMessage);

#[derive(Debug)]
struct Proxy(mpsc::Sender<Message>);
impl RawProxy for Proxy {
    fn send_message(&self, path: Arc<[ViewId]>, message: SendMessage) -> Result<(), ProxyError> {
        self.0
            .send((path, message))
            .map_err(|error| ProxyError::DriverFinished(error.0.1))
    }
    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

pub(in crate::ui) struct Worker<V: View<ShellState, (), ViewCtx, Element = NoElement>> {
    view: V,
    view_state: V::ViewState,
    ctx: ViewCtx,
    messages: mpsc::Receiver<Message>,
}

pub(in crate::ui) fn launch(
    state: &mut ShellState,
) -> Worker<impl View<ShellState, (), ViewCtx, Element = NoElement> + use<>> {
    Worker::new(crate::ui::launch::task::task(), state)
}

impl<V: View<ShellState, (), ViewCtx, Element = NoElement>> Worker<V> {
    pub(in crate::ui) fn new(view: V, state: &mut ShellState) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let (send, messages) = mpsc::channel();
        let mut ctx = ViewCtx::new(Arc::new(Proxy(send)), Arc::new(runtime));
        let (_, view_state) = view.build(&mut ctx, state);
        Self {
            view,
            view_state,
            ctx,
            messages,
        }
    }

    pub(in crate::ui) fn complete(&mut self, state: &mut ShellState) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while state
            .outputs
            .iter()
            .any(|output| output.launch_pending.is_some())
        {
            let (path, message) = self
                .messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("launch task completion");
            let mut message = MessageCtx::new(
                std::mem::take(self.ctx.environment()),
                path.to_vec(),
                message.into(),
            );
            assert!(matches!(
                self.view
                    .message(&mut self.view_state, &mut message, (), state),
                MessageResult::Action(()) | MessageResult::RequestRebuild
            ));
            let (environment, _, _) = message.finish();
            *self.ctx.environment() = environment;
        }
    }
}

impl<V: View<ShellState, (), ViewCtx, Element = NoElement>> Drop for Worker<V> {
    fn drop(&mut self) {
        self.view.teardown(&mut self.view_state, &mut self.ctx, ());
    }
}

pub(in crate::ui) fn accept(listener: &UnixListener) -> (UnixStream, String) {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "control request never reached fixture"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("accept: {error}"),
        }
    };
    listener.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut request = String::new();
    reader.read_line(&mut request).unwrap();
    (reader.into_inner(), request)
}

pub(in crate::ui) fn reply_active(listener: &UnixListener) {
    let (mut stream, request) = accept(listener);
    assert_eq!(request, "status\n");
    payload(
        &mut stream,
        r#"{"version":1,"backend":"headless","active":true,"locked":false,"recovery_ready":true}"#,
    );
}

pub(in crate::ui) fn payload(stream: &mut UnixStream, value: &str) {
    write!(stream, "ok {}\n{value}", value.len()).unwrap();
}

/// Fake only the control endpoint: keep the actual typed launch envelope,
/// payload/output assertions, worker and completion callback in the test.
pub(in crate::ui) fn read_request(stream: &mut UnixStream) -> String {
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    if let Some(envelope) =
        hearthspace_ipc::transport::launch::Envelope::decode(line.as_bytes()).unwrap()
    {
        return format!("{}\n", envelope.command);
    }
    assert!(
        !line.starts_with("launch-app") && !line.starts_with("favorite-gesture-launch "),
        "UI launches must carry their caller lifetime"
    );
    line
}
