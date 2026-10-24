use super::*;
use std::{io, sync::atomic::Ordering, thread};

#[test]
fn stalled_manager_obeys_the_absolute_startup_deadline() {
    let bus = Bus::start();
    let paths = bus.paths("timeout");
    bus.state.lock().unwrap().stall_reads = true;
    let started = Instant::now();
    let error = update_activation_environment(
        &paths,
        &bus.session(&paths),
        false,
        started + Duration::from_millis(150),
        &AtomicBool::new(false),
    )
    .err()
    .expect("deadline must fail");
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(bus.state.lock().unwrap().reads > 0);
    assert!(bus.state.lock().unwrap().replace_calls.is_empty());
    assert!(!paths.session_dir.join(cleanup::RECORD).exists());
}

#[test]
fn cancellation_interrupts_an_in_flight_manager_read() {
    let bus = Bus::start();
    let paths = bus.paths("cancel");
    bus.state.lock().unwrap().stall_reads = true;
    let cancelled = AtomicBool::new(false);
    thread::scope(|scope| {
        let worker = scope.spawn(|| {
            update_activation_environment(
                &paths,
                &bus.session(&paths),
                false,
                Instant::now() + Duration::from_secs(5),
                &cancelled,
            )
        });
        fixture::wait_until(|| bus.state.lock().unwrap().reads > 0);
        let started = Instant::now();
        cancelled.store(true, Ordering::SeqCst);
        let error = worker
            .join()
            .unwrap()
            .err()
            .expect("cancellation must fail");
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(started.elapsed() < Duration::from_secs(1));
    });
    assert!(bus.state.lock().unwrap().replace_calls.is_empty());
}

#[test]
fn waiting_for_another_publisher_uses_the_same_absolute_deadline() {
    let bus = Bus::start();
    let paths = bus.paths("locked");
    let cancelled = AtomicBool::new(false);
    let _lock = cleanup::operations::lock(
        &paths.host_runtime_dir,
        Instant::now() + Duration::from_secs(1),
        &cancelled,
    )
    .unwrap();
    let started = Instant::now();
    let error = update_activation_environment(
        &paths,
        &bus.session(&paths),
        false,
        started + Duration::from_millis(100),
        &cancelled,
    )
    .err()
    .expect("contended lock must time out");
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(bus.state.lock().unwrap().reads, 0);
}

#[test]
fn fifo_receipt_is_rejected_without_waiting_for_a_writer() {
    let bus = Bus::start();
    let paths = bus.paths("fifo");
    nix::unistd::mkfifo(
        &paths.session_dir.join(cleanup::RECORD),
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )
    .unwrap();
    let started = Instant::now();
    assert!(cleanup::retire_from_session_dir(&paths.session_dir, None).is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(bus.state.lock().unwrap().reads, 0);
}
