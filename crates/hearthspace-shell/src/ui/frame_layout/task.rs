//! A single worker serializes layout commands and coalesces semantic updates.
use std::time::{Duration, Instant};
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

use super::{
    ShellState, Snapshot, apply,
    popup_intent::{Intent, Opening},
    worker::Worker,
};

#[derive(Debug)]
enum Update {
    Snapshot(Box<Snapshot>),
    Error(String),
    Open(Opening),
}

#[derive(Debug)]
pub(super) struct Request {
    pub command: String,
    pub open: Option<Intent>,
}

pub(in crate::ui) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<>
{
    task_raw(
        |proxy: MessageProxy<Update>, state: &mut ShellState| {
            let socket = state.command_socket.clone();
            let refresh = state.frame_layout.refresh.clone();
            let (sender, mut requests) = tokio::sync::mpsc::channel::<Request>(32);
            state.frame_layout.requests = Some(sender);
            let (availability_sender, mut availability) =
                tokio::sync::watch::channel(Vec::<(String, String, bool)>::new());
            state.frame_layout.availability = Some(availability_sender);
            async move {
                let mut interval = tokio::time::interval(Duration::from_millis(500));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut previous: Option<Snapshot> = None;
                let mut worker = Worker::default();
                let mut diagnostic_id = 0_u64;
                loop {
                    let request = tokio::select! {
                        _ = interval.tick() => None,
                        _ = refresh.notified() => None,
                        _ = availability.changed() => None,
                        value = requests.recv() => { let Some(value) = value else { break }; Some(value) },
                    };
                    diagnostic_id = diagnostic_id.wrapping_add(1);
                    let available = availability.borrow_and_update().clone();
                    let path = socket.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let result = worker.poll(&path, request, available, diagnostic_id);
                        (worker, result)
                    })
                    .await;
                    let Ok((returned_worker, result)) = result else {
                        tracing::warn!(diagnostic_id, "frame layout worker task did not complete");
                        break;
                    };
                    worker = returned_worker;
                    match result {
                        Ok(snapshot) => {
                            let opening = worker.take_ready(&snapshot, Instant::now());
                            if previous.as_ref() != Some(&snapshot) {
                                previous = Some(snapshot.clone());
                                if proxy.message(Update::Snapshot(Box::new(snapshot))).is_err() {
                                    break;
                                }
                            }
                            if let Some(opening) = opening {
                                let opening_id = opening.diagnostic_id;
                                if proxy.message(Update::Open(opening)).is_err() {
                                    tracing::warn!(
                                        diagnostic_id = opening_id,
                                        "frame layout popup delivery failed"
                                    );
                                    break;
                                }
                                tracing::debug!(
                                    diagnostic_id = opening_id,
                                    "frame layout popup update enqueued"
                                );
                            }
                        }
                        Err(error) => {
                            // Retain ordinary frame parts, but never keep a stale editor modal.
                            if previous
                                .as_ref()
                                .is_some_and(|old| old.editing || old.confirmation.is_some())
                            {
                                let mut snapshot = previous.clone().unwrap_or_default();
                                snapshot.editing = false;
                                snapshot.session = 0;
                                snapshot.selected = None;
                                snapshot.confirmation = None;
                                snapshot.focus_request = None;
                                previous = Some(snapshot.clone());
                                if proxy.message(Update::Snapshot(Box::new(snapshot))).is_err() {
                                    break;
                                }
                            }
                            if proxy.message(Update::Error(error)).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        },
        apply_update,
    )
}

fn apply_update(state: &mut ShellState, update: Update) {
    match update {
        Update::Snapshot(snapshot) => {
            state.frame_layout.error = None;
            apply(state, *snapshot);
        }
        Update::Error(error) => state.frame_layout.error = Some(error),
        Update::Open(opening) => opening.deliver(state, Instant::now()),
    }
}
