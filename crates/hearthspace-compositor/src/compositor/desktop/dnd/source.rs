use hearthspace_common::desktop_files::{FileClipboard, URI_LIST};
use smithay::{
    input::dnd::{DndAction, Source, SourceMetadata},
    utils::IsAlive,
};
use std::{
    os::fd::OwnedFd,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(super) struct Files {
    files: FileClipboard,
    cancelled: AtomicBool,
}

impl Files {
    pub(super) fn new(files: FileClipboard) -> Arc<Self> {
        Arc::new(Self {
            files,
            cancelled: AtomicBool::new(false),
        })
    }
}

impl IsAlive for Files {
    fn alive(&self) -> bool {
        !self.cancelled.load(Ordering::Acquire)
    }
}

impl Source for Files {
    fn metadata(&self) -> Option<SourceMetadata> {
        self.alive().then(|| SourceMetadata {
            mime_types: vec![URI_LIST.into()],
            dnd_actions: [DndAction::Copy].into_iter().collect(),
        })
    }
    fn choose_action(&self, _: DndAction) {}
    fn send(&self, mime: &str, fd: OwnedFd) {
        if self.alive()
            && mime == URI_LIST
            && let Some(bytes) = self.files.encode(mime)
        {
            crate::compositor::clipboard::send_bytes(bytes.into(), fd);
        }
    }
    fn drop_performed(&self) {}
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    fn finished(&self) {
        self.cancel();
    }
}
