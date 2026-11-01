use super::*;
use std::{cell::Cell, rc::Rc};

fn format(code: Fourcc, modifier: Modifier) -> Format {
    Format { code, modifier }
}

#[test]
fn reverse_direct_route_wins_even_when_scanout_format_has_no_intersection() {
    // AMD renders Linear ARGB; NVIDIA can sample it. XR24 scanout must not
    // select Vulkan/CPU just because only a different copy Fourcc intersects.
    let amd = [format(Fourcc::Argb8888, Modifier::Linear)];
    let nvidia = [format(Fourcc::Argb8888, Modifier::Linear)];
    assert_eq!(
        direct_format(Fourcc::Xrgb8888, &amd, &nvidia),
        Some(Fourcc::Argb8888)
    );
}

#[test]
fn disjoint_explicit_modifiers_and_implicit_only_advertisements_do_not_claim_direct() {
    let source = [format(Fourcc::Abgr8888, Modifier::from(0x0300000000606013))];
    let target = [format(Fourcc::Abgr8888, Modifier::Linear)];
    assert_eq!(direct_format(Fourcc::Abgr8888, &source, &target), None);
    let implicit = [format(Fourcc::Abgr8888, Modifier::Invalid)];
    assert_eq!(direct_format(Fourcc::Abgr8888, &implicit, &implicit), None);
}

#[test]
fn failed_incarnations_are_not_evicted_and_recovery_cannot_grow_the_ledger() {
    let mut failures = Failures::default();
    failures.record((226, 1));
    assert!(!failures.permits(&(226, 1)));
    assert!(failures.permits(&(226, 2)));
    for incarnation in 2..=17 {
        failures.record((226, incarnation));
    }
    assert!(!failures.permits(&(227, 1000)));
    assert!(!failures.permits(&(226, 1)));
    assert_eq!(failures.keys.len(), FAILURE_LIMIT);
}

struct ExternalWork {
    ready: Rc<Cell<bool>>,
    dropped: Rc<Cell<usize>>,
}
impl Drop for ExternalWork {
    fn drop(&mut self) {
        self.dropped.set(self.dropped.get() + 1);
    }
}

#[test]
fn idle_retirement_preserves_pending_work_and_reserves_replacement_capacity() {
    let ready = Rc::new(Cell::new(false));
    let dropped = Rc::new(Cell::new(0));
    let mut slots = Slots::default();
    for key in 0..SLOT_LIMIT {
        assert!(
            slots
                .insert(
                    key,
                    ExternalWork {
                        ready: ready.clone(),
                        dropped: dropped.clone()
                    }
                )
                .is_ok()
        );
    }
    slots.retire(|_| true, |_| {});
    for _ in 0..32 {
        slots.reap(|work| work.ready.get());
    }
    assert_eq!(dropped.get(), 0);
    assert!(!slots.has_room());
    assert!(slots.active(&0).is_none());
    ready.set(true); // Substitute only the external completion query.
    slots.reap(|work| work.ready.get());
    assert_eq!(dropped.get(), SLOT_LIMIT);
    assert!(slots.has_room());
}

#[test]
fn setup_failure_falls_back_but_submitted_or_target_failure_never_does() {
    let mut attempt = Attempt::default();
    assert_eq!(attempt.finish::<(), _>(Err("unsupported import")), Ok(None));
    attempt.begin_gpu();
    assert_eq!(
        attempt.finish::<(), _>(Err("submission export failed")),
        Err("submission export failed")
    );
    assert_eq!(
        attempt.finish::<(), _>(Err("target finish failed")),
        Err("target finish failed")
    );
    assert_eq!(attempt.finish(Ok::<_, &str>(7)), Ok(Some(7)));
}

#[test]
fn failed_scanout_is_reserved_until_completion_while_another_output_stays_active() {
    let ready = Rc::new(Cell::new(false));
    let dropped = Rc::new(Cell::new(0));
    let mut slots = Slots::default();
    for output in ["failed-output", "healthy-output"] {
        assert!(
            slots
                .insert(
                    output,
                    ExternalWork {
                        ready: ready.clone(),
                        dropped: dropped.clone(),
                    }
                )
                .is_ok()
        );
    }
    let stops = Cell::new(0);
    slots.retire(
        |output| *output == "failed-output",
        |_| stops.set(stops.get() + 1),
    );
    slots.retire(
        |output| *output == "failed-output",
        |_| stops.set(stops.get() + 1),
    );
    assert_eq!(stops.get(), 1);
    assert!(slots.active(&"healthy-output").is_some());
    assert!(slots.active(&"failed-output").is_none());
    assert!(slots.retired_any(|work| !work.ready.get()));
    slots.reap(|work| work.ready.get());
    assert_eq!(dropped.get(), 0);
    ready.set(true);
    slots.reap(|work| work.ready.get());
    assert!(!slots.retired_any(|_| true));
    assert_eq!(dropped.get(), 1);
    assert!(slots.active(&"healthy-output").is_some());
}

#[test]
fn retirement_before_device_loss_rejects_pair_and_retains_unresolved_resources() {
    retired_completion_error("device lost");
}

#[test]
fn retirement_before_release_fence_error_rejects_pair_and_retains_unresolved_resources() {
    retired_completion_error("release fence query failed");
}

fn retired_completion_error(error: &'static str) {
    let ready = Rc::new(Cell::new(false));
    let dropped = Rc::new(Cell::new(0));
    let mut slots = Slots::default();
    let mut failures = Failures::default();
    let pair = ("source-incarnation-1", "target-incarnation-1");
    for output in 0..SLOT_LIMIT {
        assert!(
            slots
                .insert(
                    (pair, output),
                    ExternalWork {
                        ready: ready.clone(),
                        dropped: dropped.clone(),
                    }
                )
                .is_ok()
        );
    }
    // Submitted work is still pending when pause/rebuild retires its slots.
    poll_slots(
        &mut slots,
        &mut failures,
        |key| key.0,
        |_| Ok::<_, &str>(()),
        |_| {},
        |work| work.ready.get(),
        |_, _| {},
    );
    slots.retire(|_| true, |_| {});
    assert!(failures.permits(&pair));

    let logs = Cell::new(0);
    // Substitute the external completion error; use the same polling, failure
    // ledger, retirement, and reaping policy as BridgeState::poll.
    poll_slots(
        &mut slots,
        &mut failures,
        |key| key.0,
        |_| Err(error),
        |_| {},
        |work| work.ready.get(),
        |_, _| logs.set(logs.get() + 1),
    );
    assert!(!failures.permits(&pair));
    assert!(failures.permits(&("source-incarnation-2", "target-incarnation-1")));
    assert_eq!(logs.get(), 0); // Retirement suppresses noise, never admission failure.
    assert_eq!(dropped.get(), 0);
    assert!(!slots.has_room());
    assert!(slots.retired_any(|work| !work.ready.get()));

    // A later successful query is not permission to forget the failed pair.
    poll_slots(
        &mut slots,
        &mut failures,
        |key| key.0,
        |_| Ok::<_, &str>(()),
        |_| {},
        |work| work.ready.get(),
        |_, _| {},
    );
    assert!(!failures.permits(&pair));
    assert_eq!(dropped.get(), 0);
    ready.set(true);
    poll_slots(
        &mut slots,
        &mut failures,
        |key| key.0,
        |_| Ok::<_, &str>(()),
        |_| {},
        |work| work.ready.get(),
        |_, _| {},
    );
    assert_eq!(dropped.get(), SLOT_LIMIT);
    assert!(slots.has_room());
    assert!(!failures.permits(&pair));
}
