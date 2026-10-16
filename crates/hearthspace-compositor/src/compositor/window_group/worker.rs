//! One worker per compositor, with one replaceable queued request. Cancellation
//! never spawns another inference thread or blocks input while Ollama finishes.
use std::sync::{Arc, Condvar, Mutex, atomic::Ordering};

use hearthspace_ai::{DecisionClient, WindowCandidate};
use smithay::reexports::calloop::channel;

use super::{App, GatherRequest};

#[derive(Default)]
struct Mailbox {
    queued: Option<GatherRequest>,
    active: Option<Arc<std::sync::atomic::AtomicBool>>,
    stopped: bool,
}

#[derive(Default)]
struct Queue {
    mailbox: Mutex<Mailbox>,
    available: Condvar,
}

impl Queue {
    fn submit(&self, request: GatherRequest) {
        let mut mailbox = self
            .mailbox
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(active) = &mailbox.active {
            active.store(true, Ordering::Release);
        }
        if let Some(previous) = mailbox.queued.replace(request) {
            previous.cancel.store(true, Ordering::Release);
        }
        self.available.notify_one();
    }

    fn next(&self) -> Option<GatherRequest> {
        let mut mailbox = self
            .mailbox
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        loop {
            if mailbox.stopped {
                return None;
            }
            if let Some(request) = mailbox.queued.take() {
                mailbox.active = Some(request.cancel.clone());
                return Some(request);
            }
            mailbox = self
                .available
                .wait(mailbox)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    fn stop(&self) {
        let mut mailbox = self
            .mailbox
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        mailbox.stopped = true;
        if let Some(active) = &mailbox.active {
            active.store(true, Ordering::Release);
        }
        if let Some(request) = mailbox.queued.take() {
            request.cancel.store(true, Ordering::Release);
        }
        self.available.notify_one();
    }
}

pub(super) struct GatherWorker {
    queue: Arc<Queue>,
}

impl Drop for GatherWorker {
    fn drop(&mut self) {
        // In-flight requests have a backend deadline. Never join on the render
        // thread, including during session shutdown.
        self.queue.stop();
    }
}

impl App {
    pub(in crate::compositor) fn start_group_gather(&mut self) -> Result<(), String> {
        let request = self.begin_group_gather()?;
        if self.window_group.worker.is_none() {
            match self.create_group_worker() {
                Ok(worker) => self.window_group.worker = Some(worker),
                Err(error) => {
                    let _ = self.complete_group_gather(request.request_id, Err(error.clone()));
                    return Err(error);
                }
            }
        }
        if let Some(worker) = &self.window_group.worker {
            worker.queue.submit(request);
        }
        Ok(())
    }

    fn create_group_worker(&self) -> Result<GatherWorker, String> {
        let (sender, receiver) = channel::channel::<(u64, Result<Vec<u64>, String>)>();
        let token = self
            .loop_handle
            .insert_source(receiver, |event, _, data| {
                match event {
                    channel::Event::Msg((request_id, result)) => {
                        // The compositor revalidates the original group, title state,
                        // candidate IDs, lock state and request generation on receipt.
                        let _ = data.state.complete_group_gather(request_id, result);
                    }
                    channel::Event::Closed => {
                        // Unexpected worker exit must release the loading state
                        // and allow the next action to create a fresh worker.
                        data.state.window_group.worker.take();
                        if let Some(request_id) = data
                            .state
                            .window_group
                            .pending
                            .as_ref()
                            .map(|pending| pending.request_id)
                        {
                            let _ = data.state.complete_group_gather(
                                request_id,
                                Err("Window gathering stopped. Try again.".into()),
                            );
                        }
                    }
                }
            })
            .map_err(|error| format!("Could not create gather result channel: {error}"))?;
        let queue = Arc::new(Queue::default());
        let pending = queue.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("hearthspace-gather".into())
            .spawn(move || {
                while let Some(request) = pending.next() {
                    if request.cancel.load(Ordering::Acquire) {
                        continue;
                    }
                    let candidates = request
                        .candidates
                        .into_iter()
                        .map(|candidate| WindowCandidate {
                            id: candidate.id,
                            title: candidate.title,
                        })
                        .collect::<Vec<_>>();
                    let result = DecisionClient::from_env()
                        .and_then(|client| {
                            client.gather(&request.state, &candidates, &request.cancel, |_, _| {})
                        })
                        .map_err(|error| format!("{error:#}"));
                    if sender.send((request.request_id, result)).is_err() {
                        break;
                    }
                }
            })
        {
            self.loop_handle.remove(token);
            return Err(format!("Could not start window gathering: {error}"));
        }
        Ok(GatherWorker { queue })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn request(id: u64) -> GatherRequest {
        GatherRequest {
            request_id: id,
            state: vec!["Project".into()],
            candidates: Vec::new(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn replacement_cancels_active_and_queued_work_and_keeps_only_latest() {
        let queue = Queue::default();
        queue.submit(request(1));
        let active = queue.next().unwrap();
        let superseded = request(2);
        let superseded_cancel = superseded.cancel.clone();
        queue.submit(superseded);
        queue.submit(request(3));
        assert!(active.cancel.load(Ordering::Acquire));
        assert!(superseded_cancel.load(Ordering::Acquire));
        let latest = queue.next().unwrap();
        assert_eq!(latest.request_id, 3);
        assert!(!latest.cancel.load(Ordering::Acquire));
        queue.stop();
        assert!(latest.cancel.load(Ordering::Acquire));
        assert!(queue.next().is_none());
    }

    #[test]
    fn dropping_worker_wakes_idle_thread_and_cancels_pending_request() {
        let queue = Arc::new(Queue::default());
        let pending = request(1);
        let cancelled = pending.cancel.clone();
        queue.submit(pending);
        drop(GatherWorker {
            queue: queue.clone(),
        });
        assert!(cancelled.load(Ordering::Acquire));
        assert!(queue.next().is_none());

        let queue = Arc::new(Queue::default());
        let waiting = queue.clone();
        let thread = std::thread::spawn(move || waiting.next().is_none());
        drop(GatherWorker { queue });
        assert!(thread.join().unwrap());
    }
}
