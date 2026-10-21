//! Bounded pending input for launcher requests and explicit canvas-search sessions.
use super::{MAX_CANVAS_PENDING_KEYS, MAX_PENDING_KEYS};
use smithay::{
    backend::input::{InputTime, KeyState},
    input::keyboard::{KeyboardSource, Keycode},
};
use std::{collections::HashSet, time::Instant};

pub(in crate::compositor) struct PendingKey {
    pub source: KeyboardSource,
    pub modifiers: Option<[u32; 4]>,
    pub code: Keycode,
    pub state: KeyState,
    pub time: InputTime,
    pub(super) finish_existing: bool,
}

pub(super) struct Pending {
    pub(super) token: u64,
    pub(super) expires: Instant,
    pub(super) accepted: bool,
    pub(super) canvas: bool,
    pub(super) cancelled: bool,
    pub(super) output: Option<String>,
    pub(super) keys: Vec<PendingKey>,
    pub(super) held_before: HashSet<Keycode>,
}

#[derive(Default)]
pub(in crate::compositor) struct SearchRequest {
    pub(super) sequence: u64,
    pub(super) pending: Option<Pending>,
}

impl SearchRequest {
    pub(super) fn valid(&self) -> bool {
        self.pending.as_ref().is_some_and(|pending| {
            !pending.cancelled && (pending.canvas || Instant::now() < pending.expires)
        })
    }

    pub(super) fn accept(&mut self, token: u64) -> Result<(), String> {
        if !self.valid() {
            return Err("search request expired or cancelled".into());
        }
        let pending = self.pending.as_mut().unwrap();
        if token != pending.token || pending.accepted {
            return Err("search request superseded or already accepted".into());
        }
        pending.accepted = true;
        Ok(())
    }

    pub(super) fn take_keys(&mut self) -> Vec<PendingKey> {
        self.pending
            .take()
            .map(|pending| pending.keys)
            .unwrap_or_default()
    }

    pub(super) fn push_key(
        &mut self,
        source: KeyboardSource,
        modifiers: Option<[u32; 4]>,
        code: Keycode,
        state: KeyState,
        time: InputTime,
    ) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        let limit = if pending.canvas {
            MAX_CANVAS_PENDING_KEYS
        } else {
            MAX_PENDING_KEYS
        };
        if pending.keys.len() == limit {
            return false;
        }
        let finish_existing = state == KeyState::Released && pending.held_before.remove(&code);
        pending.keys.push(PendingKey {
            source,
            modifiers,
            code,
            state,
            time,
            finish_existing,
        });
        true
    }
}
