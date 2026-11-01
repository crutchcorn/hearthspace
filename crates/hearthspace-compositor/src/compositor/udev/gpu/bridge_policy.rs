//! Decisions shared by live transfer ownership and its deterministic regressions.
use smithay::backend::allocator::{Format, Fourcc, Modifier};

pub(super) const SLOT_LIMIT: usize = 8;
const FAILURE_LIMIT: usize = 16;

pub(in crate::compositor::udev) fn direct_format(
    preferred: Fourcc,
    render: &[Format],
    import: &[Format],
) -> Option<Fourcc> {
    [
        preferred,
        Fourcc::Argb8888,
        Fourcc::Xrgb8888,
        Fourcc::Abgr8888,
        Fourcc::Xbgr8888,
    ]
    .into_iter()
    .find(|code| {
        render.iter().any(|format| {
            format.code == *code && format.modifier != Modifier::Invalid && import.contains(format)
        })
    })
}

pub(super) struct Failures<K> {
    keys: Vec<K>,
    exhausted: bool,
}

impl<K: PartialEq> Default for Failures<K> {
    fn default() -> Self {
        Self {
            keys: Vec::with_capacity(FAILURE_LIMIT),
            exhausted: false,
        }
    }
}

impl<K: PartialEq> Failures<K> {
    pub fn permits(&self, key: &K) -> bool {
        !self.exhausted && !self.keys.contains(key)
    }

    pub fn record(&mut self, key: K) {
        if self.keys.contains(&key) {
            return;
        }
        if self.keys.len() == FAILURE_LIMIT {
            self.exhausted = true;
        } else {
            self.keys.push(key);
        }
    }
}

/// Failed setup can use Smithay immediately. Once GPU operations begin, callers
/// retire the slot and propagate the error instead of reusing uncertain storage.
#[derive(Default)]
pub(in crate::compositor::udev) struct Attempt {
    started: bool,
}

impl Attempt {
    pub fn begin_gpu(&mut self) {
        self.started = true;
    }

    pub fn finish<T, E>(&self, result: Result<T, E>) -> Result<Option<T>, E> {
        match result {
            Ok(value) => Ok(Some(value)),
            Err(_) if !self.started => Ok(None),
            Err(error) => Err(error),
        }
    }
}

pub(super) struct Entry<K, V> {
    pub key: K,
    pub value: V,
    pub retired: bool,
}

/// Active and retired entries share one budget; replacing/resizing a live slot
/// cannot silently discard pending work to make room for its replacement.
pub(super) struct Slots<K, V> {
    pub entries: Vec<Entry<K, V>>,
}

impl<K: PartialEq, V> Default for Slots<K, V> {
    fn default() -> Self {
        Self {
            entries: Vec::with_capacity(SLOT_LIMIT),
        }
    }
}

impl<K: PartialEq, V> Slots<K, V> {
    pub fn has_room(&self) -> bool {
        self.entries.len() < SLOT_LIMIT
    }

    pub fn active(&mut self, key: &K) -> Option<&mut V> {
        self.entries
            .iter_mut()
            .find(|entry| !entry.retired && &entry.key == key)
            .map(|entry| &mut entry.value)
    }

    pub fn insert(&mut self, key: K, value: V) -> Result<(), V> {
        if !self.has_room() {
            return Err(value);
        }
        self.entries.push(Entry {
            key,
            value,
            retired: false,
        });
        Ok(())
    }

    pub fn retire(&mut self, mut predicate: impl FnMut(&K) -> bool, mut stop: impl FnMut(&mut V)) {
        for entry in &mut self.entries {
            if !entry.retired && predicate(&entry.key) {
                entry.retired = true;
                stop(&mut entry.value);
            }
        }
    }

    pub fn reap(&mut self, mut complete: impl FnMut(&mut V) -> bool) {
        self.entries
            .retain_mut(|entry| !(entry.retired && complete(&mut entry.value)));
    }

    pub fn retired_any(&self, mut predicate: impl FnMut(&V) -> bool) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.retired && predicate(&entry.value))
    }
}

/// Polling remains authoritative after retirement: pause or output replacement
/// does not make a newly discovered failure safe to retry on the same pair.
/// Only external work/completion queries and diagnostics are supplied by callers.
pub(super) fn poll_slots<K: PartialEq, P: Clone + PartialEq, V, E>(
    slots: &mut Slots<K, V>,
    failures: &mut Failures<P>,
    pair: impl Fn(&K) -> P,
    mut poll: impl FnMut(&mut V) -> Result<(), E>,
    mut retire: impl FnMut(&mut V),
    complete: impl FnMut(&mut V) -> bool,
    mut log: impl FnMut(&K, &E),
) {
    let mut failed = Vec::new();
    for entry in &mut slots.entries {
        if let Err(error) = poll(&mut entry.value) {
            if !entry.retired {
                log(&entry.key, &error);
            }
            let pair = pair(&entry.key);
            failures.record(pair.clone());
            failed.push(pair);
        }
    }
    for failed_pair in failed {
        slots.retire(|key| pair(key) == failed_pair, &mut retire);
    }
    slots.reap(complete);
}

#[cfg(test)]
mod tests;
