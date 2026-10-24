//! Independent bounded admission: a slow filesystem operation cannot queue apps.
use super::{Request, ShellState, finished};
use crate::ui::control_worker;
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

const CAPACITY: usize = 4;

#[derive(Debug)]
struct Completed(Request, Result<(), String>, bool);

pub(in crate::ui) fn submit(state: &mut ShellState, request: Request) {
    let rejected = match &state.launch_requests {
        Some(sender) => sender
            .try_send(request)
            .err()
            .map(|error| error.into_inner()),
        None => Some(request),
    };
    if let Some(request) = rejected {
        finished(
            state,
            request,
            Err("Application launches are busy. Try again shortly.".into()),
            true,
        );
    }
}

pub(in crate::ui) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<>
{
    task_raw(
        |proxy: MessageProxy<Completed>, state: &mut ShellState| {
            let socket = state.command_socket.clone();
            let (sender, mut requests) = tokio::sync::mpsc::channel::<Request>(CAPACITY);
            state.launch_requests = Some(sender);
            async move {
                let mut pending = tokio::task::JoinSet::new();
                loop {
                    tokio::select! {
                        request = requests.recv(), if pending.len() < CAPACITY => {
                            let Some(request) = request else { break; };
                            let socket = socket.clone();
                            pending.spawn(async move {
                                let work = request.clone();
                                let target = socket.clone();
                                let result = tokio::task::spawn_blocking(move || work.perform(&target))
                                    .await.unwrap_or_else(|error| Err(format!("Application launch failed: {error}")));
                                let active = control_worker::session(socket).await.available;
                                Completed(request, result, active)
                            });
                        }
                        result = pending.join_next(), if !pending.is_empty() => {
                            if let Some(Ok(completed)) = result
                                && proxy.message(completed).is_err() { break; }
                        }
                    }
                }
            }
        },
        |state, Completed(request, result, active)| finished(state, request, result, active),
    )
}
