use super::{CursorUpdate, OutputConfig, OutputKey, SceneFrame};
use smithay::reexports::calloop::ping::Ping;
use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Default)]
pub(super) struct Control {
    pub generation: u64,
    pub configuration: u64,
    pub outputs: Vec<OutputConfig>,
    pub active: bool,
    pub stopped: bool,
    pub security_epoch: u64,
    pub lock_generation: Option<u64>,
}

#[derive(Default)]
pub(super) struct Updates {
    pub control: Control,
    pub scenes: HashMap<OutputKey, SceneFrame>,
    pub retired: Vec<SceneFrame>,
    pub cursor: Option<CursorUpdate>,
}

pub(super) struct Mailbox {
    state: Mutex<Updates>,
    admitted: AtomicBool,
    ping: Ping,
}

impl Mailbox {
    pub fn new(ping: Ping) -> Self {
        Self {
            state: Mutex::new(Updates {
                control: Control {
                    active: true,
                    ..Control::default()
                },
                ..Updates::default()
            }),
            admitted: AtomicBool::new(true),
            ping,
        }
    }

    pub fn configure(&self, outputs: Vec<OutputConfig>) -> u64 {
        let mut state = self.state.lock().unwrap();
        if !state.control.stopped {
            state.control.generation += 1;
            state.control.configuration = state.control.generation;
            state.control.outputs = outputs;
        }
        let generation = state.control.generation;
        drop(state);
        self.ping.ping();
        generation
    }

    pub fn submit_scene(&self, frame: SceneFrame) -> Result<(), SceneFrame> {
        let mut state = self.state.lock().unwrap();
        if !self.admitted.load(Ordering::Acquire)
            || state.control.stopped
            || frame.security_epoch != state.control.security_epoch
            || frame.lock_generation != state.control.lock_generation
            || !state
                .control
                .outputs
                .iter()
                .any(|output| output.key == frame.output)
            || state
                .retired
                .iter()
                .filter(|old| old.output == frame.output)
                .count()
                >= 3
        {
            return Err(frame);
        }
        if state
            .scenes
            .get(&frame.output)
            .is_some_and(|old| old.sequence >= frame.sequence)
        {
            return Err(frame);
        }
        if let Some(old) = state.scenes.insert(frame.output, frame) {
            // Even an unseen frame can have a producer still writing its slot.
            // Retirement is worker-owned; never drop it on the sender thread.
            state.retired.push(old);
        }
        drop(state);
        self.ping.ping();
        Ok(())
    }

    pub fn update_cursor(&self, cursor: CursorUpdate) {
        let mut state = self.state.lock().unwrap();
        if !state.control.stopped
            && cursor.security_epoch == state.control.security_epoch
            && state
                .cursor
                .as_ref()
                .is_none_or(|old| old.sequence <= cursor.sequence)
        {
            state.cursor = Some(cursor);
        }
        drop(state);
        self.ping.ping();
    }

    pub fn set_security(&self, epoch: u64, lock_generation: Option<u64>) {
        let mut state = self.state.lock().unwrap();
        if !state.control.stopped && epoch >= state.control.security_epoch {
            state.control.security_epoch = epoch;
            state.control.lock_generation = lock_generation;
        }
        drop(state);
        self.ping.ping();
    }

    pub fn set_active(&self, active: bool) -> u64 {
        // Close admission before waiting for the short mailbox critical section.
        if !active {
            self.admitted.store(false, Ordering::Release);
        }
        let mut state = self.state.lock().unwrap();
        if !state.control.stopped {
            state.control.generation += 1;
            state.control.active = active;
            if active {
                // Resume always rebuilds from the latest requested configuration.
                state.control.configuration = state.control.generation;
            }
            self.admitted.store(active, Ordering::Release);
        }
        let generation = state.control.generation;
        drop(state);
        self.ping.ping();
        generation
    }

    pub fn stop(&self) {
        self.admitted.store(false, Ordering::Release);
        let mut state = self.state.lock().unwrap();
        state.control.stopped = true;
        state.control.active = false;
        drop(state);
        self.ping.ping();
    }

    pub fn current(&self) -> Control {
        self.state.lock().unwrap().control.clone()
    }

    pub fn permits(&self, control: &Control) -> bool {
        if !self.admitted.load(Ordering::Acquire) {
            return false;
        }
        let state = self.state.lock().unwrap();
        !state.control.stopped
            && state.control.active
            && state.control.generation == control.generation
            && state.control.security_epoch == control.security_epoch
            && state.control.lock_generation == control.lock_generation
    }

    pub fn take(&self) -> Updates {
        let mut state = self.state.lock().unwrap();
        Updates {
            control: state.control.clone(),
            scenes: std::mem::take(&mut state.scenes),
            retired: std::mem::take(&mut state.retired),
            cursor: state.cursor.take(),
        }
    }
}
