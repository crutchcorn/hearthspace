use super::super::identity::DeviceToken;
use super::{
    CursorUpdate, OutputKey, PresenterEvent,
    mailbox::Mailbox,
    output::{Completion, Output, Schedule},
    render,
    retirement::{Retirement, Scene},
};
use smithay::{
    backend::{
        allocator::gbm::GbmDevice,
        drm::{DrmDevice, DrmDeviceFd, DrmEvent, DrmEventMetadata},
        egl::{EGLContext, EGLDisplay},
        renderer::gles::GlesRenderer,
    },
    reexports::{
        calloop::{EventLoop, channel::Sender, ping::PingSource},
        drm::control::crtc,
    },
    utils::Rectangle,
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) struct Worker {
    pub device: DeviceToken,
    pub drm: DrmDevice,
    pub gbm: GbmDevice<DrmDeviceFd>,
    pub renderer: GlesRenderer,
    pub outputs: HashMap<OutputKey, Output>,
    pub mailbox: Arc<Mailbox>,
    pub events: Sender<PresenterEvent>,
    pub retirement: Retirement,
    pub configured: u64,
    pub paused_ack: Option<u64>,
    pub security: (u64, Option<u64>),
    pub cursor: Option<CursorUpdate>,
    pub done: bool,
}

/// The active-output transition, before promoting a replacement scene or
/// rendering. Pending submissions retain their exact owner until completion.
pub(super) fn prepare_active_output<F>(
    key: OutputKey,
    schedule: &mut Schedule,
    scene: Option<&mut Scene<F>>,
    finish: impl FnOnce() -> bool,
    emit: impl FnOnce(PresenterEvent),
) -> bool {
    if schedule.failed || schedule.pending {
        return false;
    }
    if let Some(scene) = scene
        && scene.uncertain
    {
        // A capture failure does not invalidate a successful submission. This
        // single active recovery attempt is therefore after its completion.
        if !finish() {
            schedule.failed = true;
            emit(PresenterEvent::Failed {
                output: Some(key),
                error: "could not synchronize partial capture reads".into(),
            });
            return false;
        }
        scene.uncertain = false;
    }
    true
}

pub(super) fn run(
    device: DeviceToken,
    fd: DrmDeviceFd,
    gbm: GbmDevice<DrmDeviceFd>,
    mailbox: Arc<Mailbox>,
    wakeup: PingSource,
    events: Sender<PresenterEvent>,
) {
    if let Err(error) = run_inner(device, fd, gbm, mailbox.clone(), wakeup, events.clone()) {
        mailbox.stop();
        let _ = events.send(PresenterEvent::Failed {
            output: None,
            error,
        });
    }
    let _ = events.send(PresenterEvent::Stopped);
}

fn run_inner(
    device: DeviceToken,
    fd: DrmDeviceFd,
    gbm: GbmDevice<DrmDeviceFd>,
    mailbox: Arc<Mailbox>,
    wakeup: PingSource,
    events: Sender<PresenterEvent>,
) -> Result<(), String> {
    let mut event_loop = EventLoop::<Worker>::try_new().map_err(|error| error.to_string())?;
    let (drm, notifier) = DrmDevice::new(fd.clone(), false).map_err(|error| error.to_string())?;
    // SAFETY: The fresh EGL context belongs solely to this worker. Sharing the
    // GBM device preserves buffer bookkeeping; no current context or renderer
    // crosses a thread boundary.
    let display = unsafe { EGLDisplay::new(gbm.clone()) }.map_err(|error| error.to_string())?;
    let context = EGLContext::new(&display).map_err(|error| error.to_string())?;
    let renderer = unsafe { GlesRenderer::new(context) }.map_err(|error| error.to_string())?;
    event_loop
        .handle()
        .insert_source(wakeup, |(), &mut (), _| {})
        .map_err(|error| error.error.to_string())?;
    event_loop
        .handle()
        .insert_source(notifier, |event, metadata, worker| match event {
            DrmEvent::VBlank(crtc) => worker.vblank(crtc, metadata.as_ref()),
            DrmEvent::Error(error) => {
                worker.emit(PresenterEvent::Failed {
                    output: None,
                    error: error.to_string(),
                });
                worker.mailbox.stop();
            }
        })
        .map_err(|error| error.error.to_string())?;
    let mut worker = Worker {
        device,
        drm,
        gbm,
        renderer,
        outputs: HashMap::new(),
        mailbox,
        events,
        retirement: Retirement::default(),
        configured: 0,
        paused_ack: None,
        security: (0, None),
        cursor: None,
        done: false,
    };
    worker.emit(PresenterEvent::Ready);
    let mut dispatch_failed = false;
    while !worker.done {
        worker.turn();
        if worker.done {
            break;
        }
        if dispatch_failed {
            // A failed notifier cannot service new frames, but producer fences
            // still own accepted buffers. Retire them before releasing the FD.
            std::thread::sleep(Duration::from_millis(4));
        } else if let Err(error) = event_loop.dispatch(worker.timeout(), &mut worker) {
            worker.emit(PresenterEvent::Failed {
                output: None,
                error: error.to_string(),
            });
            worker.mailbox.stop();
            dispatch_failed = true;
        }
    }
    // Drop the event source before worker/device resources and before Stopped.
    drop(event_loop);
    drop(worker);
    Ok(())
}

impl Worker {
    pub fn emit(&self, event: PresenterEvent) {
        if self.events.send(event).is_err() {
            self.mailbox.stop();
        }
    }

    fn turn(&mut self) {
        let updates = self.mailbox.take();
        let control = updates.control;
        let events = self.events.clone();
        for frame in updates.retired {
            self.retirement.push(frame.into(), |event| {
                let _ = events.send(event);
            });
        }
        self.apply_security(&control);
        if let Some(cursor) = updates.cursor
            && cursor.security_epoch == control.security_epoch
            && self
                .cursor
                .as_ref()
                .is_none_or(|old| old.sequence <= cursor.sequence)
        {
            for output in self.outputs.values_mut() {
                if cursor.output == Some(output.config.key)
                    || output
                        .cursor
                        .as_ref()
                        .is_some_and(|old| old.output == Some(output.config.key))
                {
                    output.schedule.dirty = true;
                }
                output.cursor = Some(cursor.clone());
            }
            self.cursor = Some(cursor);
        }
        for (key, frame) in updates.scenes {
            let valid = control.active
                && !control.stopped
                && control.configuration == self.configured
                && frame.security_epoch == control.security_epoch
                && frame.lock_generation == control.lock_generation;
            let output = self
                .outputs
                .get_mut(&key)
                .filter(|output| !output.schedule.failed);
            if let Some(output) = output.filter(|_| valid) {
                let latest = output.desired.as_ref().or(output.scene.as_ref());
                if latest.is_none_or(|old| old.frame.sequence < frame.sequence) {
                    if let Some(old) = output.desired.replace(frame.into()) {
                        self.retirement.push(old, |event| {
                            let _ = events.send(event);
                        });
                    }
                    continue;
                }
            }
            self.retirement.push(frame.into(), |event| {
                let _ = events.send(event);
            });
        }
        if !self.reconcile(&control) {
            return;
        }
        if !self
            .retirement
            .recover_uncertain(|| render::finish_partial(&mut self.renderer))
        {
            self.emit(PresenterEvent::Failed {
                output: None,
                error: "could not synchronize retired scene reads".into(),
            });
            self.mailbox.stop();
            return;
        }
        self.retirement.poll(|event| {
            let _ = events.send(event);
        });
        for output in self.outputs.values_mut() {
            if !prepare_active_output(
                output.config.key,
                &mut output.schedule,
                output.scene.as_mut(),
                || render::finish_partial(&mut self.renderer),
                |event| {
                    let _ = events.send(event);
                },
            ) {
                continue;
            }
            if output
                .desired
                .as_ref()
                .is_some_and(|scene| scene.frame.ready.is_reached())
            {
                let next = output.desired.take().unwrap();
                if let Some(old) = output.scene.replace(next) {
                    self.retirement.push(old, |event| {
                        let _ = events.send(event);
                    });
                }
                let size = output.config.size;
                output
                    .scene_damage
                    .add([Rectangle::from_size((size.w, size.h).into())]);
                output.schedule.dirty = true;
            }
            if output.schedule.dirty
                && self.mailbox.permits(&control)
                && let Err(error) =
                    render::render(output, &mut self.renderer, &self.mailbox, &control)
            {
                output.schedule.failed = true;
                let _ = events.send(PresenterEvent::Failed {
                    output: Some(output.config.key),
                    error,
                });
            }
            if !output.schedule.pending
                && !output.schedule.dirty
                && output
                    .schedule
                    .opportunity_at
                    .is_some_and(|at| Instant::now() >= at)
            {
                output.schedule.opportunity_at = None;
                if self.mailbox.permits(&control) {
                    let cursor_image = output
                        .cursor
                        .as_ref()
                        .filter(|cursor| cursor.output == Some(output.config.key))
                        .and_then(|cursor| cursor.snapshot.as_ref())
                        .map(|snapshot| (snapshot.image_id.clone(), snapshot.revision));
                    let _ = events.send(PresenterEvent::FrameOpportunity {
                        key: output.config.key,
                        security_epoch: control.security_epoch,
                        cursor_image,
                    });
                }
            }
        }
    }

    fn timeout(&self) -> Option<Duration> {
        if !self.retirement.is_empty()
            || self.outputs.values().any(|output| output.desired.is_some())
        {
            return Some(Duration::from_millis(4));
        }
        self.outputs
            .values()
            .filter(|output| !output.schedule.failed && !output.schedule.pending)
            .filter_map(|output| output.schedule.opportunity_at)
            .min()
            .map(|at| at.saturating_duration_since(Instant::now()))
    }

    fn vblank(&mut self, crtc: crtc::Handle, metadata: Option<&DrmEventMetadata>) {
        let control = self.mailbox.current();
        let Some(output) = self
            .outputs
            .values_mut()
            .find(|output| output.config.key.crtc == crtc)
        else {
            return;
        };
        tracing::trace!(target: "hearthspace::presenter", key = ?output.config.key,
            pending = output.schedule.pending, ?metadata, "received native page flip");
        let completed = output.schedule.complete(metadata, || {
            output
                .compositor
                .frame_submitted()
                .map_err(|error| error.to_string())
        });
        let submission = match completed {
            Ok(Completion::Presented(submission)) => submission,
            Ok(Completion::Ignored) => return,
            Ok(Completion::Untimed) => {
                output.compositor.reset_buffer_ages();
                if let Some(scene) = output.scene.as_mut() {
                    scene.capture_taken = false;
                }
                return;
            }
            Err(error) => {
                output.schedule.failed = true;
                let _ = self.events.send(PresenterEvent::Failed {
                    output: Some(output.config.key),
                    error,
                });
                return;
            }
        };
        if !self.mailbox.permits(&control)
            || submission.security_epoch != control.security_epoch
            || submission.lock_generation != control.lock_generation
            || control.configuration != self.configured
        {
            return;
        }
        output.last_presented_scene = Some(submission.scene_sequence);
        tracing::trace!(target: "hearthspace::presenter", key = ?output.config.key,
            submission_sequence = submission.sequence, scene_sequence = submission.scene_sequence,
            cursor_sequence = submission.cursor_sequence, cursor_image = ?submission.cursor_image,
            security_epoch = submission.security_epoch, lock_generation = ?submission.lock_generation,
            cursor_plane = submission.diagnostics.cursor_plane,
            primary_scanout = submission.diagnostics.primary_scanout,
            primary_buffer = ?submission.diagnostics.primary.map(|(buffer, _)| buffer),
            primary_damage_commit = ?submission.diagnostics.primary.map(|(_, commit)| commit),
            primary_reused = submission.diagnostics.primary_reused,
            render_us = submission.diagnostics.render_us,
            completion_us = submission.diagnostics.queue_started.elapsed().as_micros() as u64,
            metadata = ?metadata, "verified native device completion");
        let _ = self.events.send(PresenterEvent::Presented {
            key: output.config.key,
            submission_sequence: submission.sequence,
            scene_sequence: submission.scene_sequence,
            cursor_sequence: submission.cursor_sequence,
            cursor_image: submission.cursor_image,
            security_epoch: submission.security_epoch,
            lock_generation: submission.lock_generation,
            metadata: *metadata.unwrap(),
            scene_changed: submission.scene_changed,
            capture: submission.capture,
        });
    }
}
