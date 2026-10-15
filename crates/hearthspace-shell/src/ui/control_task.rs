//! Reactive controls with bounded queues and measured-value OSD feedback.
use super::{
    control_state::{Action, Reading, Request, Snapshot},
    control_worker,
    state::ShellState,
};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

#[derive(Debug)]
enum Update {
    Poll(Snapshot, bool),
    Finished(Request, Result<Reading, String>, bool),
}

pub(super) fn submit(state: &mut ShellState, key: &str, action: Action, token: Option<u64>) {
    let controls = &mut state.controls;
    controls.next_id = controls.next_id.wrapping_add(1);
    let request = Request {
        id: controls.next_id,
        output: key.to_string(),
        action,
        token,
        created: Instant::now(),
    };
    let result = controls
        .sender
        .as_ref()
        .ok_or(())
        .and_then(|sender| sender.try_send(request).map_err(|_| ()));
    if result.is_ok() {
        *controls.error_mut(action.device()) = None;
        if let Some(value) = action.absolute() {
            *controls.pending_mut(action.device()) = Some((controls.next_id, value));
        }
    } else {
        *controls.error_mut(action.device()) = Some("Controls are busy. Try again shortly.".into());
    }
}

fn apply(state: &mut ShellState, update: Update) {
    match update {
        Update::Poll(snapshot, active) => {
            if snapshot.audio.is_ok() && state.controls.snapshot.audio.is_err() {
                state.controls.volume_error = None;
            }
            if matches!(snapshot.brightness, Ok(Some(_)))
                && !matches!(state.controls.snapshot.brightness, Ok(Some(_)))
            {
                state.controls.brightness_error = None;
            }
            state.controls.snapshot = snapshot;
            if !active {
                for output in &mut state.outputs {
                    output.osd = None;
                }
            }
        }
        Update::Finished(request, result, active) => {
            let pending = state.controls.pending_mut(request.action.device());
            if pending.is_some_and(|(id, _)| id <= request.id) {
                *pending = None;
            }
            match result {
                Ok(reading) => {
                    match reading {
                        Reading::Audio(value) => state.controls.snapshot.audio = Ok(value),
                        Reading::Brightness(value) => {
                            state.controls.snapshot.brightness = Ok(Some(value))
                        }
                    }
                    *state.controls.error_mut(request.action.device()) = None;
                    if active {
                        super::control_osd::show(
                            state,
                            &request.output,
                            request.action.device(),
                            None,
                        );
                    }
                }
                Err(error) => {
                    *state.controls.error_mut(request.action.device()) = Some(error.clone());
                    // A delayed result must not reopen a surface after VT loss or locking.
                    if active {
                        super::control_osd::show(
                            state,
                            &request.output,
                            request.action.device(),
                            Some(error),
                        );
                    }
                }
            }
        }
    }
}

pub(super) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Update>, state: &mut ShellState| {
            let socket = state.command_socket.clone();
            let (sender, mut commands) = tokio::sync::mpsc::channel::<Request>(32);
            state.controls.sender = Some(sender);
            async move {
                let mut queue = VecDeque::new();
                let mut next_poll = Instant::now();
                loop {
                    while queue.len() < 32
                        && let Ok(request) = commands.try_recv()
                    {
                        control_worker::coalesce(&mut queue, request);
                    }
                    if let Some(request) = queue.pop_front() {
                        let (result, authorized) = control_worker::adjust(&socket, &request).await;
                        let active =
                            authorized && control_worker::session(socket.clone()).await.available;
                        if proxy
                            .message(Update::Finished(request, result, active))
                            .is_err()
                        {
                            break;
                        }
                        continue;
                    }
                    tokio::select! {
                        biased;
                        request = commands.recv() => {
                            let Some(request) = request else { break; };
                            control_worker::coalesce(&mut queue, request);
                        }
                        (reading, active) = async {
                            tokio::time::sleep(next_poll.saturating_duration_since(Instant::now())).await;
                            let session = control_worker::session(socket.clone()).await;
                            (control_worker::snapshot(session.native).await, session.available)
                        } => {
                            if proxy.message(Update::Poll(reading, active)).is_err() { break; }
                            next_poll = Instant::now() + Duration::from_secs(1);
                        }
                    }
                }
            }
        },
        apply,
    )
}

#[cfg(test)]
#[path = "control_task_tests.rs"]
mod tests;
