//! One coalescing worker publishes live X11 state; launch workers await their own
//! snapshot before asking the compositor for final admission.
#[cfg(feature = "xwayland")]
use hearthspace_desktop::activation::operations::TIMEOUT;
use hearthspace_desktop::activation::{DisplayUpdate, MARKER};
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "xwayland")]
use std::{
    sync::{Arc, Condvar, Mutex, OnceLock, atomic::AtomicBool},
    time::Instant,
};

static REVISION: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "xwayland")]
type Job = (DisplayUpdate, Arc<AtomicBool>);
#[cfg(feature = "xwayland")]
struct Queue {
    pending: Option<Job>,
    active: Option<Arc<AtomicBool>>,
}
#[cfg(feature = "xwayland")]
struct Worker {
    queue: Mutex<Queue>,
    ready: Condvar,
}
#[cfg(feature = "xwayland")]
static WORKER: OnceLock<Result<Arc<Worker>, String>> = OnceLock::new();

pub(super) fn snapshot(display: Option<String>) -> Option<DisplayUpdate> {
    let generation = std::env::var(MARKER)
        .ok()
        .filter(|value| !value.is_empty())?;
    let session = std::env::var_os(hearthspace_common::SESSION_DIR_ENV)?;
    Some(DisplayUpdate {
        session: session.into(),
        generation,
        revision: REVISION.load(Ordering::Acquire),
        display,
    })
}

pub(super) fn current(update: &DisplayUpdate, display: Option<String>) -> bool {
    update.revision == REVISION.load(Ordering::Acquire) && update.display == display
}

#[cfg(feature = "xwayland")]
fn worker() -> Result<&'static Arc<Worker>, String> {
    WORKER
        .get_or_init(|| {
            let worker = Arc::new(Worker {
                queue: Mutex::new(Queue {
                    pending: None,
                    active: None,
                }),
                ready: Condvar::new(),
            });
            let state = worker.clone();
            std::thread::Builder::new()
                .name("activation-display".into())
                .spawn(move || {
                    loop {
                        let (update, cancelled) = {
                            let mut queue = state.queue.lock().unwrap();
                            while queue.pending.is_none() {
                                queue = state.ready.wait(queue).unwrap();
                            }
                            let job = queue.pending.take().unwrap();
                            queue.active = Some(job.1.clone());
                            job
                        };
                        if let Err(error) = update.publish(Instant::now() + TIMEOUT, &cancelled)
                            && !cancelled.load(Ordering::Acquire)
                        {
                            tracing::warn!(%error, "could not publish live X11 display");
                        }
                        state.queue.lock().unwrap().active = None;
                    }
                })
                .map_err(|error| error.to_string())?;
            Ok(worker)
        })
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(feature = "xwayland")]
impl super::App {
    pub(in crate::compositor) fn publish_xwayland_display(&self) {
        REVISION.fetch_add(1, Ordering::AcqRel);
        let Some(update) = snapshot(self.xwayland_display_name()) else {
            return;
        };
        let worker = match worker() {
            Ok(worker) => worker,
            Err(error) => {
                tracing::warn!(%error, "could not start X11 publication worker");
                return;
            }
        };
        let mut queue = worker.queue.lock().unwrap();
        if let Some(active) = &queue.active {
            active.store(true, Ordering::Release);
        }
        queue.pending = Some((update, Arc::new(AtomicBool::new(false))));
        worker.ready.notify_one();
    }
}
