//! Contain an observed import-error FD ownership violation without closing a
//! descriptor that the driver may have closed and another thread may have reused.
use super::BridgeError;
use ash::vk;
use std::{
    os::fd::{IntoRawFd, OwnedFd, RawFd},
    sync::Mutex,
};

pub(super) static IMPORTS: ImportGate = ImportGate::new();

pub(super) struct ImportGate(Mutex<bool>);

impl ImportGate {
    const fn new() -> Self {
        Self(Mutex::new(true))
    }

    pub fn is_open(&self) -> bool {
        self.0.lock().is_ok_and(|open| *open)
    }

    pub fn allocate<T>(
        &self,
        fd: OwnedFd,
        allocate: impl FnOnce(RawFd) -> Result<T, vk::Result>,
    ) -> Result<T, BridgeError> {
        let mut open = self.0.lock().map_err(|_| BridgeError::Retired)?;
        if !*open {
            return Err(BridgeError::Retired);
        }
        // The lock serializes admission through the external call, even if a
        // future consumer creates independent bridges on several threads.
        // Success transfers ownership under Vulkan's contract. Failure in the
        // observed NVIDIA/layer stack closed this FD despite that contract.
        // No destructor may run on the now-ambiguous descriptor number.
        let raw = fd.into_raw_fd();
        match allocate(raw) {
            Ok(memory) => Ok(memory),
            Err(result) => {
                *open = false;
                // Deliberately never close/reconstruct/probe raw. A conforming
                // failure may leave one FD and its allocation until process exit.
                // Closing admission bounds this to ONE ambiguous FD per process.
                Err(BridgeError::ImportOwnershipUncertain(result))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::Cell,
        os::{fd::FromRawFd, unix::net::UnixStream},
    };

    fn fd() -> OwnedFd {
        let (a, _b) = UnixStream::pair().unwrap();
        a.into()
    }

    #[test]
    fn driver_closed_fd_on_failure_cannot_double_close_or_reopen_admission() {
        let gate = ImportGate::new();
        let result = gate.allocate::<()>(fd(), |raw| {
            // Substitute the external driver's nonconforming ownership action.
            drop(unsafe { OwnedFd::from_raw_fd(raw) });
            Err(vk::Result::ERROR_OUT_OF_DEVICE_MEMORY)
        });
        assert!(matches!(
            result,
            Err(BridgeError::ImportOwnershipUncertain(_))
        ));
        let called = Cell::new(false);
        for _ in 0..32 {
            assert!(
                gate.allocate::<()>(fd(), |_| {
                    called.set(true);
                    Ok(())
                })
                .is_err()
            );
        }
        assert!(!called.get());
        assert!(!gate.is_open());
    }

    #[test]
    fn conforming_failure_quarantines_one_fd_and_success_keeps_gate_open() {
        let gate = ImportGate::new();
        gate.allocate(fd(), |raw| {
            drop(unsafe { OwnedFd::from_raw_fd(raw) });
            Ok(())
        })
        .unwrap();
        assert!(gate.is_open());
        let retained = Cell::new(-1);
        assert!(
            gate.allocate::<()>(fd(), |raw| {
                retained.set(raw);
                Err(vk::Result::ERROR_OUT_OF_DEVICE_MEMORY)
            })
            .is_err()
        );
        assert!(!gate.is_open());
        // Only this substitute knows the FD was not consumed. Production must
        // never perform this cleanup on an ambiguous driver-owned number.
        drop(unsafe { OwnedFd::from_raw_fd(retained.get()) });
    }

    #[test]
    fn independently_created_bridges_share_serialized_failure_admission() {
        use std::sync::{
            Arc, Barrier,
            atomic::{AtomicUsize, Ordering},
        };
        let gate = Arc::new(ImportGate::new());
        let start = Arc::new(Barrier::new(8));
        let calls = Arc::new(AtomicUsize::new(0));
        let threads = (0..8)
            .map(|_| {
                let (gate, start, calls) = (gate.clone(), start.clone(), calls.clone());
                std::thread::spawn(move || {
                    let fd = fd();
                    start.wait();
                    assert!(
                        gate.allocate::<()>(fd, |raw| {
                            calls.fetch_add(1, Ordering::SeqCst);
                            drop(unsafe { OwnedFd::from_raw_fd(raw) });
                            Err(vk::Result::ERROR_OUT_OF_DEVICE_MEMORY)
                        })
                        .is_err()
                    );
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(!gate.is_open());
    }
}
