//! Supervisor-only cancellation; cleanup always stays in the owning thread.
use std::{
    io,
    sync::atomic::{AtomicI32, Ordering},
};

static CANCELLED: AtomicI32 = AtomicI32::new(0);

extern "C" fn request(signal: libc::c_int) {
    // Native atomic store only: no allocation, I/O, locking, or unwinding in
    // the signal handler. The supervisor polls it alongside its deadline.
    CANCELLED.store(signal, Ordering::Relaxed);
}

pub struct Cancellation {
    previous: Vec<(libc::c_int, libc::sigaction)>,
}

impl Cancellation {
    pub fn install() -> io::Result<Self> {
        CANCELLED.store(0, Ordering::Relaxed);
        let mut guard = Self {
            previous: Vec::new(),
        };
        // Caught dispositions reset on exec, so workers retain their original
        // termination behavior and parent-death fallback.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = request as *const () as libc::sighandler_t;
            action.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            for signal in [libc::SIGTERM, libc::SIGINT] {
                let mut previous = std::mem::zeroed();
                if libc::sigaction(signal, &action, &mut previous) != 0 {
                    return Err(io::Error::last_os_error());
                }
                guard.previous.push((signal, previous));
            }
        }
        Ok(guard)
    }

    pub fn exit_code(&self) -> Option<i32> {
        match CANCELLED.load(Ordering::Relaxed) {
            0 => None,
            signal => Some(128 + signal),
        }
    }
}

impl Drop for Cancellation {
    fn drop(&mut self) {
        for (signal, previous) in self.previous.iter().rev() {
            unsafe {
                libc::sigaction(*signal, previous, std::ptr::null_mut());
            }
        }
    }
}
