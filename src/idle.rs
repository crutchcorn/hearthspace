use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub type WindowId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowIdleState {
    Active,
    Idle(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityReason {
    ClientInput,
    SurfaceCommit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleTransitionReason {
    Activity(ActivityReason),
    ThresholdElapsed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdleTransition {
    pub window_id: WindowId,
    pub from: WindowIdleState,
    pub to: WindowIdleState,
    pub at: Instant,
    pub reason: IdleTransitionReason,
}

pub struct WindowIdleDaemon {
    command_tx: Sender<IdleCommand>,
    transition_rx: Receiver<IdleTransition>,
    thread: Option<JoinHandle<()>>,
}

impl WindowIdleDaemon {
    pub fn new(thresholds: impl Into<Vec<Duration>>) -> Self {
        let thresholds = thresholds.into();
        let (command_tx, command_rx) = mpsc::channel();
        let (transition_tx, transition_rx) = mpsc::channel();
        let thread = thread::spawn(move || run_idle_daemon(thresholds, command_rx, transition_tx));

        Self {
            command_tx,
            transition_rx,
            thread: Some(thread),
        }
    }

    pub fn register_window(&self, window_id: WindowId) {
        let _ = self.command_tx.send(IdleCommand::RegisterWindow(window_id));
    }

    pub fn unregister_window(&self, window_id: WindowId) {
        let _ = self
            .command_tx
            .send(IdleCommand::UnregisterWindow(window_id));
    }

    pub fn record_activity(&self, window_id: WindowId, reason: ActivityReason) {
        let _ = self
            .command_tx
            .send(IdleCommand::RecordActivity(window_id, reason));
    }

    pub fn drain_transitions(&self) -> Vec<IdleTransition> {
        let mut transitions = Vec::new();
        while let Ok(transition) = self.transition_rx.try_recv() {
            transitions.push(transition);
        }
        transitions
    }
}

impl Drop for WindowIdleDaemon {
    fn drop(&mut self) {
        let _ = self.command_tx.send(IdleCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Debug)]
enum IdleCommand {
    RegisterWindow(WindowId),
    UnregisterWindow(WindowId),
    RecordActivity(WindowId, ActivityReason),
    Shutdown,
}

fn run_idle_daemon(
    thresholds: Vec<Duration>,
    command_rx: Receiver<IdleCommand>,
    transition_tx: Sender<IdleTransition>,
) {
    let mut core = IdleTrackerCore::new(thresholds);
    let mut deadlines = BinaryHeap::new();

    loop {
        fire_due_deadlines(&mut core, &mut deadlines, &transition_tx, Instant::now());

        let command = match next_timeout(&deadlines, Instant::now()) {
            Some(timeout) => match command_rx.recv_timeout(timeout) {
                Ok(command) => command,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match command_rx.recv() {
                Ok(command) => command,
                Err(_) => break,
            },
        };

        if !handle_command(command, &mut core, &mut deadlines, &transition_tx) {
            break;
        }
    }
}

fn fire_due_deadlines(
    core: &mut IdleTrackerCore,
    deadlines: &mut BinaryHeap<ScheduledIdleDeadline>,
    transition_tx: &Sender<IdleTransition>,
    now: Instant,
) {
    while deadlines.peek().is_some_and(|deadline| deadline.at <= now) {
        let Some(deadline) = deadlines.pop() else {
            return;
        };
        let (transition, next_deadline) = core.fire_deadline(deadline);
        send_transition(transition_tx, transition);
        push_deadline(deadlines, next_deadline);
    }
}

fn handle_command(
    command: IdleCommand,
    core: &mut IdleTrackerCore,
    deadlines: &mut BinaryHeap<ScheduledIdleDeadline>,
    transition_tx: &Sender<IdleTransition>,
) -> bool {
    let now = Instant::now();
    match command {
        IdleCommand::RegisterWindow(window_id) => {
            push_deadline(deadlines, core.register_window(window_id, now));
            compact_deadlines_if_needed(deadlines, core);
        }
        IdleCommand::UnregisterWindow(window_id) => core.unregister_window(window_id),
        IdleCommand::RecordActivity(window_id, reason) => {
            let (transition, deadline) = core.record_activity(window_id, now, reason);
            send_transition(transition_tx, transition);
            push_deadline(deadlines, deadline);
            compact_deadlines_if_needed(deadlines, core);
        }
        IdleCommand::Shutdown => return false,
    }

    true
}

fn next_timeout(deadlines: &BinaryHeap<ScheduledIdleDeadline>, now: Instant) -> Option<Duration> {
    deadlines
        .peek()
        .map(|deadline| deadline.at.saturating_duration_since(now))
}

fn push_deadline(
    deadlines: &mut BinaryHeap<ScheduledIdleDeadline>,
    deadline: Option<ScheduledIdleDeadline>,
) {
    if let Some(deadline) = deadline {
        deadlines.push(deadline);
    }
}

fn compact_deadlines_if_needed(
    deadlines: &mut BinaryHeap<ScheduledIdleDeadline>,
    core: &IdleTrackerCore,
) {
    let max_stale_deadlines = core.window_count().saturating_mul(4).max(64);
    if deadlines.len() <= max_stale_deadlines {
        return;
    }

    deadlines.clear();
    deadlines.extend(core.pending_deadlines());
}

fn send_transition(transition_tx: &Sender<IdleTransition>, transition: Option<IdleTransition>) {
    if let Some(transition) = transition {
        let _ = transition_tx.send(transition);
    }
}

#[derive(Debug)]
struct IdleTrackerCore {
    thresholds: Vec<Duration>,
    windows: HashMap<WindowId, WindowIdleRecord>,
}

impl IdleTrackerCore {
    fn new(thresholds: Vec<Duration>) -> Self {
        Self {
            thresholds,
            windows: HashMap::new(),
        }
    }

    fn register_window(
        &mut self,
        window_id: WindowId,
        now: Instant,
    ) -> Option<ScheduledIdleDeadline> {
        let generation = self
            .windows
            .get(&window_id)
            .map_or(0, |record| record.generation + 1);
        self.windows.insert(
            window_id,
            WindowIdleRecord {
                state: WindowIdleState::Active,
                generation,
                stage_started_at: now,
                pending_deadline: deadline_for(&self.thresholds, window_id, generation, 0, now),
            },
        );
        self.windows
            .get(&window_id)
            .and_then(|record| record.pending_deadline)
    }

    fn unregister_window(&mut self, window_id: WindowId) {
        self.windows.remove(&window_id);
    }

    fn record_activity(
        &mut self,
        window_id: WindowId,
        now: Instant,
        reason: ActivityReason,
    ) -> (Option<IdleTransition>, Option<ScheduledIdleDeadline>) {
        let Some(record) = self.windows.get_mut(&window_id) else {
            return (None, None);
        };

        let from = record.state;
        record.state = WindowIdleState::Active;
        record.generation += 1;
        record.stage_started_at = now;
        let generation = record.generation;
        let deadline = deadline_for(&self.thresholds, window_id, generation, 0, now);
        record.pending_deadline = deadline;

        let transition = (from != WindowIdleState::Active).then_some(IdleTransition {
            window_id,
            from,
            to: WindowIdleState::Active,
            at: now,
            reason: IdleTransitionReason::Activity(reason),
        });

        (transition, deadline)
    }

    fn fire_deadline(
        &mut self,
        deadline: ScheduledIdleDeadline,
    ) -> (Option<IdleTransition>, Option<ScheduledIdleDeadline>) {
        let Some(record) = self.windows.get_mut(&deadline.window_id) else {
            return (None, None);
        };

        if record.generation != deadline.generation
            || record.pending_deadline != Some(deadline)
            || expected_next_level(record.state) != Some(deadline.target_level)
        {
            return (None, None);
        }

        let from = record.state;
        let to = WindowIdleState::Idle(deadline.target_level);
        record.state = to;
        record.stage_started_at = deadline.at;
        let next_level = deadline.target_level + 1;
        let next_deadline = deadline_for(
            &self.thresholds,
            deadline.window_id,
            deadline.generation,
            next_level,
            deadline.at,
        );
        record.pending_deadline = next_deadline;

        let transition = IdleTransition {
            window_id: deadline.window_id,
            from,
            to,
            at: deadline.at,
            reason: IdleTransitionReason::ThresholdElapsed,
        };

        (Some(transition), next_deadline)
    }

    fn window_count(&self) -> usize {
        self.windows.len()
    }

    fn pending_deadlines(&self) -> impl Iterator<Item = ScheduledIdleDeadline> + '_ {
        self.windows
            .values()
            .filter_map(|record| record.pending_deadline)
    }
}

fn deadline_for(
    thresholds: &[Duration],
    window_id: WindowId,
    generation: u64,
    target_level: usize,
    from: Instant,
) -> Option<ScheduledIdleDeadline> {
    thresholds
        .get(target_level)
        .map(|threshold| ScheduledIdleDeadline {
            at: from + *threshold,
            window_id,
            generation,
            target_level,
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowIdleRecord {
    state: WindowIdleState,
    generation: u64,
    stage_started_at: Instant,
    pending_deadline: Option<ScheduledIdleDeadline>,
}

fn expected_next_level(state: WindowIdleState) -> Option<usize> {
    match state {
        WindowIdleState::Active => Some(0),
        WindowIdleState::Idle(level) => Some(level + 1),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScheduledIdleDeadline {
    at: Instant,
    window_id: WindowId,
    generation: u64,
    target_level: usize,
}

impl Ord for ScheduledIdleDeadline {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .at
            .cmp(&self.at)
            .then_with(|| other.window_id.cmp(&self.window_id))
            .then_with(|| other.generation.cmp(&self.generation))
            .then_with(|| other.target_level.cmp(&self.target_level))
    }
}

impl PartialOrd for ScheduledIdleDeadline {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW_1: WindowId = 1;
    const WINDOW_2: WindowId = 2;

    fn thresholds() -> Vec<Duration> {
        vec![
            Duration::from_secs(5),
            Duration::from_secs(10),
            Duration::from_secs(30),
        ]
    }

    #[test]
    fn new_window_starts_active_and_schedules_first_level() {
        let mut core = IdleTrackerCore::new(thresholds());
        let now = Instant::now();

        let deadline = core.register_window(WINDOW_1, now).unwrap();

        assert_eq!(core.windows[&WINDOW_1].state, WindowIdleState::Active);
        assert_eq!(deadline.window_id, WINDOW_1);
        assert_eq!(deadline.target_level, 0);
        assert_eq!(deadline.at, now + Duration::from_secs(5));
    }

    #[test]
    fn first_idle_level_transitions_after_threshold() {
        let mut core = IdleTrackerCore::new(thresholds());
        let now = Instant::now();
        let deadline = core.register_window(WINDOW_1, now).unwrap();

        let (transition, next_deadline) = core.fire_deadline(deadline);

        assert_eq!(
            transition.unwrap(),
            IdleTransition {
                window_id: WINDOW_1,
                from: WindowIdleState::Active,
                to: WindowIdleState::Idle(0),
                at: now + Duration::from_secs(5),
                reason: IdleTransitionReason::ThresholdElapsed,
            }
        );
        assert_eq!(next_deadline.unwrap().at, now + Duration::from_secs(15));
    }

    #[test]
    fn later_idle_levels_are_measured_from_previous_level() {
        let mut core = IdleTrackerCore::new(thresholds());
        let now = Instant::now();
        let first = core.register_window(WINDOW_1, now).unwrap();
        let (_, second) = core.fire_deadline(first);
        let second = second.unwrap();

        assert_eq!(second.target_level, 1);
        assert_eq!(second.at, first.at + Duration::from_secs(10));

        let (_, third) = core.fire_deadline(second);
        let third = third.unwrap();
        assert_eq!(third.target_level, 2);
        assert_eq!(third.at, second.at + Duration::from_secs(30));
    }

    #[test]
    fn windows_advance_independently() {
        let mut core = IdleTrackerCore::new(thresholds());
        let now = Instant::now();
        let window_1_deadline = core.register_window(WINDOW_1, now).unwrap();
        let window_2_deadline = core
            .register_window(WINDOW_2, now + Duration::from_secs(2))
            .unwrap();

        core.fire_deadline(window_1_deadline);

        assert_eq!(core.windows[&WINDOW_1].state, WindowIdleState::Idle(0));
        assert_eq!(core.windows[&WINDOW_2].state, WindowIdleState::Active);
        assert_eq!(window_2_deadline.at, now + Duration::from_secs(7));
    }

    #[test]
    fn activity_resets_only_target_window() {
        let mut core = IdleTrackerCore::new(thresholds());
        let now = Instant::now();
        let first = core.register_window(WINDOW_1, now).unwrap();
        let second = core.register_window(WINDOW_2, now).unwrap();
        core.fire_deadline(first);
        core.fire_deadline(second);

        let activity_at = now + Duration::from_secs(6);
        let (transition, deadline) =
            core.record_activity(WINDOW_1, activity_at, ActivityReason::ClientInput);

        assert_eq!(transition.unwrap().from, WindowIdleState::Idle(0));
        assert_eq!(transition.unwrap().to, WindowIdleState::Active);
        assert_eq!(deadline.unwrap().at, activity_at + Duration::from_secs(5));
        assert_eq!(core.windows[&WINDOW_1].state, WindowIdleState::Active);
        assert_eq!(core.windows[&WINDOW_2].state, WindowIdleState::Idle(0));
    }

    #[test]
    fn active_window_activity_schedules_new_generation_without_transition() {
        let mut core = IdleTrackerCore::new(thresholds());
        let now = Instant::now();
        let old_deadline = core.register_window(WINDOW_1, now).unwrap();
        let activity_at = now + Duration::from_secs(2);

        let (transition, new_deadline) =
            core.record_activity(WINDOW_1, activity_at, ActivityReason::SurfaceCommit);
        let (stale_transition, _) = core.fire_deadline(old_deadline);

        assert!(transition.is_none());
        assert!(stale_transition.is_none());
        assert_eq!(
            new_deadline.unwrap().at,
            activity_at + Duration::from_secs(5)
        );
    }

    #[test]
    fn unregistered_window_stops_transitioning() {
        let mut core = IdleTrackerCore::new(thresholds());
        let now = Instant::now();
        let deadline = core.register_window(WINDOW_1, now).unwrap();

        core.unregister_window(WINDOW_1);
        let (transition, next_deadline) = core.fire_deadline(deadline);

        assert!(transition.is_none());
        assert!(next_deadline.is_none());
    }

    #[test]
    fn due_deadline_processing_emits_all_crossed_levels_in_order() {
        let mut core = IdleTrackerCore::new(vec![
            Duration::from_secs(1),
            Duration::from_secs(1),
            Duration::from_secs(1),
        ]);
        let mut deadlines = BinaryHeap::new();
        let (transition_tx, transition_rx) = mpsc::channel();
        let now = Instant::now();
        deadlines.push(core.register_window(WINDOW_1, now).unwrap());

        fire_due_deadlines(
            &mut core,
            &mut deadlines,
            &transition_tx,
            now + Duration::from_secs(3),
        );

        let transitions = transition_rx.try_iter().collect::<Vec<_>>();
        assert_eq!(transitions.len(), 3);
        assert_eq!(transitions[0].to, WindowIdleState::Idle(0));
        assert_eq!(transitions[1].to, WindowIdleState::Idle(1));
        assert_eq!(transitions[2].to, WindowIdleState::Idle(2));
        assert!(deadlines.is_empty());
    }
}
