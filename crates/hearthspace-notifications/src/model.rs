//! Bounded notification state. Application text is data, never executable content.
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Serialize;

const MAX_ACTIVE: usize = 16;
const MAX_RETAINED: usize = 24;
const MAX_EVENTS: usize = 128;
pub(crate) type Shared = Arc<Mutex<State>>;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Action {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Notification {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<Action>,
    pub urgency: u8,
    pub active: bool,
    pub close_reason: Option<u32>,
    #[serde(skip)]
    sender: String,
    #[serde(skip)]
    deadline: Option<Instant>,
    #[serde(skip)]
    resident: bool,
    #[serde(skip)]
    transient: bool,
}

pub(crate) struct Incoming<'a> {
    pub sender: &'a str,
    pub replaces_id: u32,
    pub app_name: &'a str,
    pub summary: &'a str,
    pub body: &'a str,
    pub actions: &'a [String],
    pub urgency: u8,
    pub resident: bool,
    pub transient: bool,
    pub expire_timeout: i32,
}

pub(crate) enum Event {
    Closed {
        sender: String,
        id: u32,
        reason: u32,
    },
    Action {
        sender: String,
        id: u32,
        key: String,
    },
}

#[derive(Default)]
pub(crate) struct State {
    entries: VecDeque<Notification>,
    events: VecDeque<Event>,
    next_id: u32,
}

impl State {
    pub fn notify(&mut self, input: Incoming<'_>, now: Instant) -> Result<u32, String> {
        if input.expire_timeout < -1 || !input.actions.len().is_multiple_of(2) {
            return Err("Invalid expiry or unpaired action key/label".into());
        }
        // Action keys must remain exact for truthful ActionInvoked delivery.
        if input.actions.len() > 8
            || input.actions.as_chunks::<2>().0.iter().any(|pair| {
                pair[0].is_empty() || pair[0].len() > 32 || pair[0].chars().any(char::is_control)
            })
        {
            return Err(
                "At most four actions with nonempty keys of at most 32 bytes are supported".into(),
            );
        }
        let replacing = self
            .entries
            .iter()
            .position(|entry| entry.active && entry.id == input.replaces_id);
        if let Some(index) = replacing {
            if self.entries[index].sender != input.sender {
                return Err("Notification belongs to another application".into());
            }
        } else if self.entries.iter().filter(|entry| entry.active).count() >= MAX_ACTIVE {
            return Err("Too many active notifications".into());
        }
        let id = if let Some(index) = replacing {
            self.entries.remove(index).unwrap().id
        } else {
            self.next_id = self
                .next_id
                .checked_add(1)
                .ok_or("Notification identifiers exhausted")?;
            self.next_id
        };
        // Retain recent closed notifications, but never silently evict live ones.
        if self.entries.len() >= MAX_RETAINED {
            let oldest_closed = self
                .entries
                .iter()
                .rposition(|entry| !entry.active)
                .unwrap();
            self.entries.remove(oldest_closed);
        }
        let timeout = match input.expire_timeout {
            0 => None,
            -1 if input.urgency == 2 => None,
            -1 => Some(Duration::from_secs(8)),
            millis => Some(Duration::from_millis(millis as u64)),
        };
        self.entries.push_front(Notification {
            id,
            app_name: text(input.app_name, 64),
            summary: text(input.summary, 128),
            body: text(input.body, 512),
            actions: input
                .actions
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| Action {
                    key: pair[0].clone(),
                    label: text(&pair[1], 48),
                })
                .collect(),
            urgency: input.urgency.min(2),
            active: true,
            close_reason: None,
            sender: input.sender.into(),
            deadline: timeout.map(|timeout| now + timeout),
            resident: input.resident,
            transient: input.transient,
        });
        Ok(id)
    }

    pub fn close(&mut self, id: u32, reason: u32, sender: Option<&str>) -> Result<(), String> {
        self.reserve_events(1)?;
        let index = self
            .entries
            .iter()
            .position(|entry| entry.id == id && entry.active)
            .ok_or("Notification is no longer active")?;
        let entry = &mut self.entries[index];
        if sender.is_some_and(|sender| entry.sender != sender) {
            return Err("Notification belongs to another application".into());
        }
        self.events.push_back(Event::Closed {
            sender: entry.sender.clone(),
            id,
            reason,
        });
        entry.active = false;
        entry.close_reason = Some(reason);
        entry.actions.clear();
        if entry.transient {
            self.entries.remove(index);
        }
        Ok(())
    }

    pub fn invoke(&mut self, id: u32, key: &str) -> Result<(), String> {
        self.reserve_events(2)?;
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.id == id && entry.active)
            .ok_or("Notification is no longer active")?;
        if !entry.actions.iter().any(|action| action.key == key) {
            return Err("Unknown notification action".into());
        }
        let resident = entry.resident;
        self.events.push_back(Event::Action {
            sender: entry.sender.clone(),
            id,
            key: key.into(),
        });
        if !resident {
            self.close(id, 2, None)?;
        }
        Ok(())
    }

    pub fn expire(&mut self, now: Instant) {
        let expired: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| entry.active && entry.deadline.is_some_and(|deadline| now >= deadline))
            .map(|entry| entry.id)
            .collect();
        for id in expired {
            let _ = self.close(id, 1, None);
        }
    }

    pub fn list(&self) -> Vec<Notification> {
        self.entries.iter().cloned().collect()
    }
    pub fn take_events(&mut self) -> Vec<Event> {
        self.events.drain(..).collect()
    }
    fn reserve_events(&self, count: usize) -> Result<(), String> {
        if self.events.len() + count > MAX_EVENTS {
            Err("Notification service is busy".into())
        } else {
            Ok(())
        }
    }
}

fn text(value: &str, max_bytes: usize) -> String {
    let mut result = String::new();
    for character in value.chars().filter(|c| !c.is_control() || *c == '\n') {
        if result.len() + character.len_utf8() > max_bytes {
            break;
        }
        result.push(character);
    }
    result
}

#[cfg(test)]
mod tests;
