use smithay::{
    backend::allocator::{Buffer, Format},
    reexports::calloop::EventLoop,
};
use wayland_server::DisplayHandle;

use super::super::FeedbackState;
use crate::compositor::{App, CalloopData, frame_callbacks::test_support::app::new_app};

pub(super) use crate::compositor::client::ClientState as ClientDataState;

pub(super) struct Server {
    pub(super) app: App,
    feedback: FeedbackState,
    pub(super) imported: Vec<Format>,
    _event_loop: EventLoop<'static, CalloopData>,
}

impl Server {
    pub(super) fn new(display: &DisplayHandle) -> Self {
        let event_loop = EventLoop::try_new().unwrap();
        Self {
            app: new_app(display, event_loop.handle()),
            feedback: FeedbackState::default(),
            imported: Vec::new(),
            _event_loop: event_loop,
        }
    }

    pub(super) fn replace(
        &mut self,
        display: &DisplayHandle,
        device: u64,
        formats: Vec<Format>,
    ) -> std::io::Result<()> {
        self.feedback.refresh::<App>(
            display,
            &mut self.app.dmabuf_state,
            &mut self.app._dmabuf_global,
            &mut self.app.dmabuf_feedback_surfaces,
            device,
            formats,
        )
    }

    pub(super) fn reject_imports(&mut self) {
        // Actual App protocol dispatch queues these. Substitute only GPU import
        // at its asynchronous drain boundary, preserving the nonfatal response.
        for (dmabuf, notifier) in self.app.pending_dmabuf_imports.drain(..) {
            self.imported.push(dmabuf.format());
            notifier.failed();
        }
    }
}
