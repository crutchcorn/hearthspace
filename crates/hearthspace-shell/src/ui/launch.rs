//! Deferred launch feedback belongs to the output and interaction that requested it.
//! A bounded launch task runs independently of desktop filesystem operations.
use super::state::ShellState;
use hearthspace_ipc::transport::launch::Cancellation;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub(super) mod task;

static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub(super) struct Pending {
    id: u64,
    cancelled: Arc<Cancellation>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancelled.cancel();
    }
}

#[derive(Clone, Debug)]
pub(super) struct Request {
    id: u64,
    output: String,
    command: String,
    deadline: Instant,
    drawing: Option<u64>,
    dock: bool,
    cancelled: Arc<Cancellation>,
}

impl Request {
    pub(super) fn perform(&self, socket: &Path) -> Result<(), String> {
        if self.cancelled.is_cancelled() {
            return Err("Application launch was cancelled.".into());
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or("Application launch expired before it could be confirmed.")?;
        super::command_client::send_launch_until(
            socket,
            &self.command,
            self.deadline,
            &self.cancelled,
        )
    }
}

pub(super) fn app(
    state: &mut ShellState,
    key: &str,
    app_id: &str,
    output: Option<&str>,
    dock: bool,
) {
    let command = match output {
        Some(output) => crate::ShellCommand::LaunchAppOnOutput {
            app_id: app_id.into(),
            output_name: output.into(),
        },
        None => crate::ShellCommand::LaunchApp(app_id.into()),
    };
    submit(state, key, command.wire_name(), None, dock);
}

pub(super) fn submit(
    state: &mut ShellState,
    key: &str,
    command: String,
    drawing: Option<u64>,
    dock: bool,
) {
    let Some(output) = state.outputs.iter_mut().find(|output| output.key() == key) else {
        return;
    };
    if output.launch_pending.is_some() {
        return;
    } // One accepted click per interaction.
    if drawing.is_some_and(|generation| generation != output.drawing.generation) {
        return;
    }
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let cancelled = Arc::new(Cancellation::default());
    output.launch_pending = Some(Pending {
        id,
        cancelled: cancelled.clone(),
    });
    output.launch_error = None;
    task::submit(
        state,
        Request {
            id,
            output: key.into(),
            command,
            deadline: Instant::now() + Duration::from_secs(10),
            drawing,
            dock,
            cancelled,
        },
    );
}

pub(super) fn finished(
    state: &mut ShellState,
    request: Request,
    result: Result<(), String>,
    active: bool,
) {
    let Some(output) = state.outputs.iter_mut().find(|output| {
        output.key() == request.output
            && output
                .launch_pending
                .as_ref()
                .is_some_and(|pending| pending.id == request.id)
    }) else {
        return;
    };
    let current = !request.cancelled.is_cancelled()
        && request
            .drawing
            .is_none_or(|generation| output.drawing.generation == generation);
    output.launch_pending = None;
    if !active || !current {
        return;
    }
    match result {
        Ok(()) => state.dismiss_launcher(&request.output),
        Err(error) => {
            // A dock request has no open palette yet. Show its failure where
            // users can choose another app; ordinary launcher queries stay intact.
            if request.dock {
                output.show_search = true;
            }
            output.launch_error = Some(error);
        }
    }
}

#[cfg(test)]
pub(super) mod tests;
