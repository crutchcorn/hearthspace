use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
};

use zeroize::Zeroizing;

use super::{Failure, Identity};

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Completion {
    pub(crate) generation: u64,
    pub(crate) result: Result<(), Failure>,
}

pub(crate) enum Poll {
    Pending,
    Complete(Completion),
}

/// A per-attempt channel avoids retaining a password queue. Session state permits
/// only one Attempt at a time; no GTK object ever crosses this boundary.
pub(crate) struct Attempt {
    generation: u64,
    receiver: Receiver<Completion>,
}

impl Attempt {
    pub(crate) fn start(
        generation: u64,
        identity: Identity,
        password: Zeroizing<String>,
    ) -> Result<Self, Failure> {
        Self::spawn(generation, move || super::authenticate(&identity, password))
    }

    fn spawn(
        generation: u64,
        work: impl FnOnce() -> Result<(), Failure> + Send + 'static,
    ) -> Result<Self, Failure> {
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("lock-authentication".into())
            .spawn(move || {
                // A panic drops the sender; poll converts channel loss to failure.
                // A late result after shutdown owns no windows or session state.
                let result = work();
                let _ = sender.send(Completion { generation, result });
            })
            .map_err(|_| Failure::Unavailable)?;
        Ok(Self {
            generation,
            receiver,
        })
    }

    pub(crate) fn poll(&self) -> Poll {
        match self.receiver.try_recv() {
            Ok(completion) => Poll::Complete(completion),
            Err(TryRecvError::Empty) => Poll::Pending,
            Err(TryRecvError::Disconnected) => Poll::Complete(Completion {
                generation: self.generation,
                result: Err(Failure::Unavailable),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_result_preserves_generation_and_account_denial() {
        let attempt = Attempt::spawn(9, || Err(Failure::Denied)).unwrap();
        assert_eq!(
            attempt.receiver.recv().unwrap(),
            Completion {
                generation: 9,
                result: Err(Failure::Denied),
            }
        );
    }

    #[test]
    fn a_panicked_worker_cannot_report_authentication_success() {
        let attempt = Attempt::spawn(12, || panic!("synthetic worker failure")).unwrap();
        assert!(attempt.receiver.recv().is_err());
        assert!(matches!(
            attempt.poll(),
            Poll::Complete(Completion {
                generation: 12,
                result: Err(Failure::Unavailable)
            })
        ));
    }

    #[test]
    fn dropping_the_receiver_does_not_leave_a_blocked_sender() {
        let (finished, receiver) = mpsc::channel();
        let (release, barrier) = mpsc::channel();
        let attempt = Attempt::spawn(2, move || {
            barrier.recv().unwrap();
            finished.send(()).unwrap();
            Ok(())
        })
        .unwrap();
        drop(attempt);
        release.send(()).unwrap();
        receiver.recv().unwrap();
    }
}
