//! Scripted external rendering/KMS operations for the production retry controller.
use super::super::{
    CapturedFrame, OutputKey, PresenterEvent,
    mailbox::{Control, Mailbox},
    output::{RenderOutcome, Submission, SubmissionDiagnostics, SubmissionOps},
    retirement::{Retained, Retirement, Scene},
};
use super::{AttemptError, AttemptOps, render_and_queue};
use crate::compositor::udev::identity::DeviceToken;
use smithay::{
    backend::{
        drm::{DrmError, DrmEventMetadata, DrmEventTime, compositor::FrameFlags},
        renderer::{
            element::Id,
            sync::{Fence, Interrupted, SyncPoint},
        },
    },
    reexports::calloop::ping::{PingSource, make_ping},
    utils::{Clock, Monotonic},
};
use std::{
    collections::VecDeque,
    io::{self, ErrorKind},
    num::NonZeroU32,
    os::fd::OwnedFd,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Instant,
};

pub(super) fn key() -> OutputKey {
    OutputKey {
        device: DeviceToken {
            id: 226,
            incarnation: 2,
        },
        crtc: NonZeroU32::new(7).unwrap().into(),
        generation: 3,
    }
}

pub(super) fn metadata() -> DrmEventMetadata {
    DrmEventMetadata {
        time: DrmEventTime::Monotonic(Clock::<Monotonic>::new().now().into()),
        sequence: 19,
    }
}

pub(super) enum Reject {
    Access(ErrorKind),
    TestFailed,
    Inactive,
}

impl Reject {
    fn error(self, selected: bool) -> AttemptError {
        match self {
            // AccessError has private fields in Smithay; use the same access
            // classifier that AttemptError::queue uses on its public source.
            Self::Access(kind) => {
                AttemptError::access(selected, &io::Error::from(kind), format!("{kind:?}"))
            }
            Self::TestFailed => AttemptError::queue(selected, DrmError::TestFailed(key().crtc)),
            Self::Inactive => AttemptError::queue(selected, DrmError::DeviceInactive),
        }
    }
}

pub(super) enum Step {
    Queue {
        selected: bool,
        rejection: Option<Reject>,
    },
    RenderError,
    Empty,
}

pub(super) fn rejected(rejection: Reject) -> Step {
    Step::Queue {
        selected: true,
        rejection: Some(rejection),
    }
}

pub(super) fn accepted() -> Step {
    Step::Queue {
        selected: false,
        rejection: None,
    }
}

#[derive(Clone, Copy)]
pub(super) enum Revoke {
    Pause,
    Security,
    Configure,
    Stop,
}

impl Revoke {
    fn apply(self, mailbox: &Mailbox) {
        match self {
            Self::Pause => {
                mailbox.set_active(false);
            }
            Self::Security => mailbox.set_security(1, Some(2)),
            Self::Configure => {
                mailbox.configure(Vec::new());
            }
            Self::Stop => mailbox.stop(),
        }
    }
}

pub(super) struct Lease(Arc<AtomicUsize>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }
}

#[derive(Debug)]
struct OrderedReadFence(Vec<Arc<AtomicBool>>);
impl Fence for OrderedReadFence {
    fn is_signaled(&self) -> bool {
        self.0.iter().all(|done| done.load(Ordering::Acquire))
    }
    fn wait(&self) -> Result<(), Interrupted> {
        panic!("test must not wait on unfinished reads")
    }
    fn is_exportable(&self) -> bool {
        false
    }
    fn export(&self) -> Option<OwnedFd> {
        None
    }
}

pub(super) struct DeviceOps {
    pub steps: VecDeque<Step>,
    pub attempts: Vec<FrameFlags>,
    pub resets: usize,
    pub primary_scanout: bool,
    pub state_resets: usize,
    pub queued_sequences: Vec<u64>,
    pub rejected_sequences: Vec<u64>,
    pub captures: usize,
    pub capture_error: Option<usize>,
    pub accepted: Option<Submission>,
    pub image_id: Id,
    pub scene: Option<Scene<Lease>>,
    pub drops: Arc<AtomicUsize>,
    pub reads: Vec<Arc<AtomicBool>>,
    pub revoke_after_rejection: Option<Revoke>,
    pub revoke_on_reset: Option<Revoke>,
    pub revoke_before_retry_queue: Option<Revoke>,
    mailbox: Arc<Mailbox>,
    control: Control,
    producer: SyncPoint,
    _ping_source: PingSource,
}

impl DeviceOps {
    pub fn new(steps: impl IntoIterator<Item = Step>) -> Self {
        let (ping, source) = make_ping().unwrap();
        let mailbox = Arc::new(Mailbox::new(ping));
        let drops = Arc::new(AtomicUsize::new(0));
        Self {
            steps: steps.into_iter().collect(),
            attempts: Vec::new(),
            resets: 0,
            primary_scanout: false,
            state_resets: 0,
            queued_sequences: Vec::new(),
            rejected_sequences: Vec::new(),
            captures: 0,
            capture_error: None,
            accepted: None,
            image_id: Id::new(),
            scene: Some(Scene {
                frame: Lease(drops.clone()),
                texture: None,
                read_sync: None,
                capture_taken: false,
                submitted: false,
                uncertain: false,
            }),
            drops,
            reads: Vec::new(),
            revoke_after_rejection: None,
            revoke_on_reset: None,
            revoke_before_retry_queue: None,
            control: mailbox.current(),
            mailbox,
            producer: SyncPoint::signaled(),
            _ping_source: source,
        }
    }

    pub fn retire(mut self) -> Retirement<Scene<Lease>> {
        let scene = self.scene.take().unwrap();
        let mut retirement = Retirement::default();
        retirement.retain(
            Retained {
                key: key(),
                sequence: 41,
                submitted: scene.submitted,
                producer: self.producer.clone(),
                reader: scene.read_sync.clone(),
                uncertain: scene.uncertain,
                owner: scene,
            },
            |event| {
                assert!(matches!(
                    event,
                    PresenterEvent::SceneDiscarded {
                        scene_sequence: 41,
                        ..
                    }
                ))
            },
        );
        retirement
    }
}

impl SubmissionOps for DeviceOps {
    fn reset_state(&mut self) -> Result<(), String> {
        self.state_resets += 1;
        Ok(())
    }
    fn render_and_queue(&mut self) -> Result<RenderOutcome, String> {
        render_and_queue(self)
    }
}

impl AttemptOps for DeviceOps {
    fn permitted(&self) -> bool {
        self.mailbox.permits(&self.control)
    }
    fn reset_buffer_ages(&mut self) {
        self.resets += 1;
        if let Some(revoke) = self.revoke_on_reset.take() {
            revoke.apply(&self.mailbox);
        }
    }

    fn attempt(&mut self, flags: FrameFlags) -> Result<RenderOutcome, AttemptError> {
        assert!(self.permitted());
        assert!(
            self.producer.is_reached(),
            "native promotion requires a ready producer"
        );
        assert_eq!(self.drops.load(Ordering::Acquire), 0);
        self.attempts.push(flags);
        let (selected, rejection) = match self.steps.pop_front().expect("unexpected third attempt")
        {
            Step::Queue {
                selected,
                rejection,
            } => (selected, rejection),
            Step::RenderError => {
                return Err("render failed with InvalidInput text (not a queue error)"
                    .to_owned()
                    .into());
            }
            Step::Empty => return Ok(RenderOutcome::Empty),
        };
        if flags.is_empty() {
            assert!(!selected, "software attempt cannot select a cursor plane");
        }
        assert!(self.accepted.is_none(), "queued twice before completion");
        // Fresh readback values identify the exact attempt. No retry algorithm
        // lives here: this operation either returns one rejection or queues one frame.
        self.captures += 1;
        self.reads.push(Arc::new(AtomicBool::new(false)));
        let scene = self.scene.as_mut().unwrap();
        scene.read_sync = Some(SyncPoint::from(OrderedReadFence(self.reads.clone())));
        assert!(
            !scene.capture_taken,
            "rejected attempt consumed capture ownership"
        );
        let capture = if self.capture_error == Some(self.captures) {
            scene.capture_result(Err("retry readback failed".into()), || false)
        } else {
            Ok(CapturedFrame {
                size: (1, 1).into(),
                pixels: vec![self.captures as u8; 4],
                clean_pixels: vec![0; 4],
            })
        };
        if self.attempts.len() == 2
            && let Some(revoke) = self.revoke_before_retry_queue.take()
        {
            revoke.apply(&self.mailbox);
        }
        if !self.permitted() {
            return Ok(RenderOutcome::Revoked);
        }
        let sequence = self.queued_sequences.len() as u64 + 1;
        self.queued_sequences.push(sequence);
        let submission = Submission {
            sequence,
            scene_sequence: 41,
            cursor_sequence: 72,
            cursor_image: Some((self.image_id.clone(), 9)),
            security_epoch: 0,
            lock_generation: None,
            scene_changed: true,
            capture: Some(capture),
            diagnostics: SubmissionDiagnostics {
                cursor_plane: selected && !self.primary_scanout,
                primary_scanout: selected && self.primary_scanout,
                window_id: Some(42),
                primary: None,
                primary_reused: false,
                render_us: 1,
                queue_started: Instant::now(),
            },
        };
        if let Some(rejection) = rejection {
            self.rejected_sequences.push(sequence);
            drop(submission);
            if let Some(revoke) = self.revoke_after_rejection.take() {
                revoke.apply(&self.mailbox);
            }
            return Err(rejection.error(selected));
        }
        let scene = self.scene.as_mut().unwrap();
        scene.submitted = true;
        scene.capture_taken = true;
        self.accepted = Some(submission);
        Ok(RenderOutcome::Queued)
    }
}
