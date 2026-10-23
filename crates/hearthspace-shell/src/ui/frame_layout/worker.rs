//! Serialized compositor work; retry only observation of an acknowledged popup.
use std::{collections::BTreeMap, io::ErrorKind, path::Path, time::Instant};

use super::{Snapshot, popup_intent::Opening, task::Request};
use crate::ui::command_client;

#[derive(Default)]
pub(super) struct Worker {
    published_availability: BTreeMap<(String, String), bool>,
    pending: Option<Opening>,
    dispatched: Option<Opening>,
}

impl Worker {
    fn cancel_pending(&mut self) {
        if let Some(opening) = self.pending.take() {
            opening.intent.cancel();
        }
    }

    fn cancel_openings(&mut self) {
        self.cancel_pending();
        if let Some(opening) = self.dispatched.take() {
            opening.intent.cancel();
        }
    }

    fn prune(&mut self, now: Instant) {
        for opening in [&mut self.pending, &mut self.dispatched] {
            if opening
                .as_ref()
                .is_some_and(|opening| !opening.valid_at(now))
            {
                *opening = None;
            }
        }
    }

    pub fn poll(
        &mut self,
        path: &Path,
        request: Option<Request>,
        available: Vec<(String, String, bool)>,
        diagnostic_id: u64,
    ) -> Result<Snapshot, String> {
        self.prune(Instant::now());
        for (output, id, available) in available {
            let key = (output.clone(), id.clone());
            if self.published_availability.get(&key) != Some(&available) {
                let command = format!(
                    "frame-widget-available {}",
                    serde_json::json!({"output":output,"id":id,"available":available})
                );
                if command_client::send_command(path, &command).is_ok() {
                    self.published_availability.insert(key, available);
                }
            }
        }
        if let Some(request) = request
            && request
                .open
                .as_ref()
                .is_none_or(|intent| intent.is_current())
        {
            let started = Instant::now();
            let ack = command_client::send_command(path, &request.command);
            tracing::debug!(target: "hearthspace_shell::ui::frame_layout::task", diagnostic_id, popup_pending = request.open.is_some(),
                acknowledged = ack.is_ok(), elapsed_ms = started.elapsed().as_millis() as u64,
                "frame layout request acknowledgement");
            if let Err(error) = ack {
                if let Some(intent) = request.open {
                    intent.cancel();
                }
                self.cancel_openings();
                return Err(error);
            }
            if let Some(intent) = request.open {
                self.cancel_pending();
                // Granted once, only after acknowledgement; polling never renews it.
                self.pending = Some(intent.acknowledge(diagnostic_id, Instant::now()));
            }
        }
        let started = Instant::now();
        let query = command_client::query_frame_layout(path);
        tracing::debug!(target: "hearthspace_shell::ui::frame_layout::task", diagnostic_id, popup_pending = self.pending.is_some(),
            succeeded = query.is_ok(), elapsed_ms = started.elapsed().as_millis() as u64,
            "frame layout query completed");
        let bytes = query.map_err(|error| {
            let transient = matches!(error.kind(),
                ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted);
            if !transient { self.cancel_openings(); }
            self.prune(Instant::now());
            tracing::warn!(target: "hearthspace_shell::ui::frame_layout::task", diagnostic_id, error_kind = ?error.kind(),
                popup_retained = self.pending.is_some(),
                "frame layout query failed");
            error.to_string()
        })?;
        let parsed = parse(&bytes);
        // An Open can still be queued on the UI thread. Invalidate its shared
        // ticket now, before the newer snapshot joins that same message queue.
        if parsed.as_ref().is_ok_and(|snapshot| {
            self.dispatched
                .as_ref()
                .is_some_and(|opening| !opening.eligible(snapshot))
        }) && let Some(opening) = self.dispatched.take()
        {
            opening.intent.cancel();
        }
        if parsed.is_err() {
            self.cancel_openings();
            tracing::warn!(target: "hearthspace_shell::ui::frame_layout::task", diagnostic_id, "frame layout snapshot validation failed");
        }
        parsed
    }

    pub fn take_ready(&mut self, snapshot: &Snapshot, now: Instant) -> Option<Opening> {
        let opening = self.pending.take()?;
        if opening.valid_at(now) && opening.eligible(snapshot) {
            self.dispatched = Some(opening.clone());
            Some(opening)
        } else {
            opening.intent.cancel();
            tracing::debug!(target: "hearthspace_shell::ui::frame_layout::task", diagnostic_id = opening.diagnostic_id,
                "frame layout pending popup cancelled or expired");
            None
        }
    }
}

pub(super) fn parse(bytes: &[u8]) -> Result<Snapshot, String> {
    let snapshot: Snapshot = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if snapshot.version != 1
        || snapshot.outputs.len() > 64
        || (snapshot.editing && snapshot.session == 0)
    {
        return Err("Invalid frame layout state".into());
    }
    for output in &snapshot.outputs {
        output.layout.validate().map_err(str::to_string)?;
        if output.widgets.len() > hearthspace_common::frame_layout::WIDGETS.len()
            || output.groups.len() > 1024
        {
            return Err("Invalid frame layout parts".into());
        }
        for rect in std::iter::once(output.rect)
            .chain(output.widgets.iter().map(|part| part.rect))
            .chain(
                output
                    .groups
                    .iter()
                    .flat_map(|group| [group.rect, group.grip]),
            )
        {
            if ![rect.x, rect.y, rect.width, rect.height]
                .into_iter()
                .all(f64::is_finite)
                || rect.width < 0.0
                || rect.height < 0.0
            {
                return Err("Invalid frame layout geometry".into());
            }
        }
    }
    Ok(snapshot)
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
