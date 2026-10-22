//! Request lifecycle diagnostics deliberately exclude keycodes and query text.
use std::{panic::Location, time::Instant};

use super::SearchRequest;

impl SearchRequest {
    #[track_caller]
    pub(super) fn trace(
        &self,
        event: &str,
        token: Option<u64>,
        output: Option<&str>,
        reason: &str,
    ) {
        let now = Instant::now();
        let pending = self.pending.as_ref();
        tracing::debug!(
            target: "hearthspace_compositor::launcher_request",
            event,
            reason,
            caller = %Location::caller(),
            ?token,
            pending_token = ?pending.map(|pending| pending.token),
            token_matches = ?token.map(|token| pending.is_some_and(|pending| pending.token == token)),
            ?output,
            pending_output = ?pending.and_then(|pending| pending.output.as_deref()),
            output_matches = ?(event == "mapping").then(|| pending.is_some_and(|pending| pending.output.as_deref() == output)),
            accepted = pending.is_some_and(|pending| pending.accepted),
            canvas = pending.is_some_and(|pending| pending.canvas),
            cancelled = pending.is_some_and(|pending| pending.cancelled),
            expired = pending.is_some_and(|pending| !pending.canvas && now >= pending.expires),
            remaining_ms = ?pending.map(|pending| pending.expires.saturating_duration_since(now).as_millis()),
            overdue_ms = ?pending.map(|pending| now.saturating_duration_since(pending.expires).as_millis()),
            buffered_keys = pending.map_or(0, |pending| pending.keys.len()),
            "launcher request lifecycle",
        );
        // Superseded tokens and explicit cancellations are routine. A current,
        // accepted surface missing its startup budget warrants a visible warning.
        if let Some(pending) = pending
            && event == "mapping"
            && reason == "rejected"
            && pending.accepted
            && !pending.canvas
            && !pending.cancelled
            && now >= pending.expires
            && Some(pending.token) == token
            && pending.output.as_deref() == output
        {
            tracing::warn!(
                target: "hearthspace_compositor::launcher_request",
                ?token,
                ?output,
                overdue_ms = now.saturating_duration_since(pending.expires).as_millis(),
                buffered_keys = pending.keys.len(),
                "launcher first buffer missed startup deadline",
            );
        }
    }
}
