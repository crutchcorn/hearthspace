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
