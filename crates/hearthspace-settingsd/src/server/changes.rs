//! Directory observation survives atomic replacement of the preference inode.
use nix::{
    errno::Errno,
    sys::inotify::{AddWatchFlags, InitFlags, Inotify, WatchDescriptor},
};
use std::{
    fs, io,
    os::fd::{AsFd, BorrowedFd},
    path::Path,
};

pub(super) struct Changes {
    inotify: Inotify,
    watch: Option<WatchDescriptor>,
}
impl Changes {
    pub(super) fn new(path: &Path) -> io::Result<Self> {
        let mut changes = Self {
            inotify: Inotify::init(InitFlags::IN_CLOEXEC | InitFlags::IN_NONBLOCK)?,
            watch: None,
        };
        changes.drain(path)?;
        Ok(changes)
    }
    pub(super) fn drain(&mut self, path: &Path) -> io::Result<()> {
        loop {
            match self.inotify.read_events() {
                Ok(events) => {
                    if events.is_empty() {
                        break;
                    }
                    for event in events {
                        if event.mask.intersects(
                            AddWatchFlags::IN_IGNORED
                                | AddWatchFlags::IN_DELETE_SELF
                                | AddWatchFlags::IN_MOVE_SELF,
                        ) && let Some(watch) = self.watch.take()
                        {
                            let _ = self.inotify.rm_watch(watch);
                        }
                    }
                }
                Err(Errno::EAGAIN) => break,
                Err(Errno::EINTR) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        if self.watch.is_none() {
            let parent = super::super::storage::parent(path);
            fs::create_dir_all(parent)?;
            self.watch = Some(self.inotify.add_watch(
                parent,
                AddWatchFlags::IN_CLOSE_WRITE
                    | AddWatchFlags::IN_MOVED_TO
                    | AddWatchFlags::IN_CREATE
                    | AddWatchFlags::IN_DELETE
                    | AddWatchFlags::IN_DELETE_SELF
                    | AddWatchFlags::IN_MOVE_SELF,
            )?);
        }
        Ok(())
    }
}
impl AsFd for Changes {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.inotify.as_fd()
    }
}
