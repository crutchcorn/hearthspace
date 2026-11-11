//! Keep independent fixtures serialized even when an earlier test panics.
use std::sync::{Mutex, MutexGuard};

pub(super) fn lock_after_cleanup(lock: &Mutex<()>) -> MutexGuard<'_, ()> {
    lock.lock().unwrap_or_else(|poisoned| {
        // This lock protects no data. Fixture guards clean up their processes
        // and environment before releasing it during unwinding. Recovering it
        // lets the next independent fixture run; the original test still fails.
        lock.clear_poison();
        poisoned.into_inner()
    })
}

#[test]
fn failed_fixture_keeps_its_failure_without_blocking_later_serialized_fixtures() {
    use std::{
        sync::{
            TryLockError,
            atomic::{AtomicBool, Ordering},
        },
        thread,
    };

    struct Cleanup<'a>(&'a AtomicBool);

    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let lock = Mutex::new(());
    let cleaned_up = AtomicBool::new(false);
    let failure = thread::scope(|scope| {
        scope
            .spawn(|| {
                let _guard = lock_after_cleanup(&lock);
                let _cleanup = Cleanup(&cleaned_up);
                panic!("intentional fixture failure");
            })
            .join()
    });
    assert!(failure.is_err(), "the original failure must propagate");
    assert!(cleaned_up.load(Ordering::SeqCst));
    assert!(lock.is_poisoned());

    let recovered_guard = lock_after_cleanup(&lock);
    assert!(!lock.is_poisoned());
    thread::scope(|scope| {
        scope
            .spawn(|| {
                assert!(matches!(lock.try_lock(), Err(TryLockError::WouldBlock)));
            })
            .join()
            .unwrap();
    });
    drop(recovered_guard);

    thread::scope(|scope| {
        scope
            .spawn(|| {
                let _next_fixture = lock_after_cleanup(&lock);
                assert!(cleaned_up.load(Ordering::SeqCst));
            })
            .join()
            .unwrap();
    });
}

#[test]
fn failed_compositor_retains_the_observed_frame_and_command_reply() {
    use super::*;
    let (sender, receiver) = std::sync::mpsc::channel();
    let failure = thread::spawn(move || {
        let _guard = headless_test_guard();
        let mut compositor = HeadlessCompositor::spawn();
        drop(compositor.wait_for_socket());
        assert_eq!(
            compositor.send_text_command("pointer-motion-abs 20 30"),
            "ok\n"
        );
        let frame = compositor.take_screenshot();
        sender
            .send((compositor.runtime_dir.path().to_owned(), frame))
            .unwrap();
        panic!("intentional fixture failure after observing a rendered frame");
    })
    .join();
    assert!(
        failure.is_err(),
        "the original assertion failure must propagate"
    );
    let (directory, observed) = receiver.recv().unwrap();
    assert_eq!(
        std::fs::read(directory.join("last-observed.png")).unwrap(),
        observed
    );
    let commands: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("control-tail.json")).unwrap())
            .unwrap();
    assert_eq!(commands[0]["command"], "pointer-motion-abs 20 30");
    assert_eq!(commands[0]["reply"], "ok\n");
    // This intentional failure's evidence has been checked; real failures stay
    // retained for the E2E runner to archive.
    std::fs::remove_dir_all(directory).unwrap();
    let _guard = headless_test_guard();
    let mut next = HeadlessCompositor::spawn();
    drop(next.wait_for_socket());
    assert!(next.take_screenshot().starts_with(b"\x89PNG\r\n\x1a\n"));
}
