use super::{
    SceneStorage, capture,
    fallback::{self, AttemptError, AttemptOps},
    mailbox::{Control, Mailbox},
    output::{Output, RenderOutcome, Submission, SubmissionDiagnostics, SubmissionOps},
    scene_element::SceneElement,
};
use smithay::{
    backend::{
        drm::compositor::{FrameError, FrameFlags, PrimaryPlaneElement},
        renderer::{
            Color32F, ImportDma, Renderer,
            element::{
                Kind, memory::MemoryRenderBufferRenderElement, render_elements,
                texture::TextureRenderElement,
            },
            gles::GlesRenderer,
        },
    },
    utils::{Rectangle, Transform},
};
use std::time::Instant;

render_elements! {
    pub(super) PresenterElement<=GlesRenderer>;
    Scene=SceneElement,
    Cursor=MemoryRenderBufferRenderElement<GlesRenderer>,
}

/// Only an error path uses glFinish. It fences partial commands for which
/// Smithay could not return a completion. Failure retains the affected lease.
pub(super) fn finish_partial(renderer: &mut GlesRenderer) -> bool {
    renderer
        .with_context(|gl| {
            // SAFETY: with_context makes this worker's context current. Finish
            // changes no GL state and waits for all prior commands on that context.
            unsafe {
                gl.Finish();
            }
        })
        .is_ok()
}

pub(super) fn render(
    output: &mut Output,
    renderer: &mut GlesRenderer,
    mailbox: &Mailbox,
    control: &Control,
) -> Result<(), String> {
    let mut schedule = std::mem::take(&mut output.schedule);
    let refresh = output.refresh;
    let rendered = schedule.submit(
        Instant::now(),
        refresh,
        &mut NativeSubmission {
            output,
            renderer,
            mailbox,
            control,
        },
    );
    output.schedule = schedule;
    if rendered.is_err() {
        let complete = finish_partial(renderer);
        if let Some(scene) = output.scene.as_mut() {
            scene.uncertain |= !complete;
        }
    }
    rendered
}

struct NativeSubmission<'a> {
    output: &'a mut Output,
    renderer: &'a mut GlesRenderer,
    mailbox: &'a Mailbox,
    control: &'a Control,
}

impl SubmissionOps for NativeSubmission<'_> {
    fn reset_state(&mut self) -> Result<(), String> {
        if !self.mailbox.permits(self.control) {
            return Ok(());
        }
        self.output
            .compositor
            .reset_state()
            .map_err(|error| error.to_string())
    }

    fn render_and_queue(&mut self) -> Result<RenderOutcome, String> {
        fallback::render_and_queue(self)
    }
}

impl AttemptOps for NativeSubmission<'_> {
    fn permitted(&self) -> bool {
        self.mailbox.permits(self.control)
    }

    fn reset_buffer_ages(&mut self) {
        tracing::trace!(target: "hearthspace::presenter", key = ?self.output.config.key,
            rejected_submission = self.output.next_submission,
            "preparing one composited plane retry");
        self.output.compositor.reset_buffer_ages();
    }

    fn attempt(&mut self, flags: FrameFlags) -> Result<RenderOutcome, AttemptError> {
        prepare_and_queue(
            self.output,
            self.renderer,
            self.mailbox,
            self.control,
            flags,
        )
    }
}

fn prepare_and_queue(
    output: &mut Output,
    renderer: &mut GlesRenderer,
    mailbox: &Mailbox,
    control: &Control,
    flags: FrameFlags,
) -> Result<RenderOutcome, AttemptError> {
    if !mailbox.permits(control) {
        return Ok(RenderOutcome::Revoked);
    }
    let mut elements = Vec::<PresenterElement>::new();
    let cursor = output.cursor.as_ref().filter(|cursor| {
        cursor.output == Some(output.config.key) && cursor.security_epoch == control.security_epoch
    });
    let cursor_sequence = cursor.map_or(0, |cursor| cursor.sequence);
    let snapshot = cursor.and_then(|cursor| cursor.snapshot.as_ref());
    output
        .compositor
        .set_cursor_hotspot(snapshot.map(|snapshot| snapshot.hotspot));
    let cursor_image = snapshot.map(|snapshot| (snapshot.image_id.clone(), snapshot.revision));
    if let Some(snapshot) = snapshot {
        elements.push(
            output
                .cursor_cache
                .element(renderer, snapshot, output.cursor_caps)?
                .into(),
        );
    }

    let scene_sequence = output
        .scene
        .as_ref()
        .map_or(0, |scene| scene.frame.sequence);
    if let Some(scene) = output.scene.as_mut() {
        if scene.texture.is_none() {
            scene.texture = Some(
                renderer
                    .import_dmabuf(&scene.frame.dmabuf, None)
                    .map_err(|error| error.to_string())?,
            );
        }
        let size = output.config.size;
        elements.push(
            SceneElement {
                texture: TextureRenderElement::from_texture_with_damage(
                    output.scene_id.clone(),
                    renderer.context_id(),
                    (0.0, 0.0),
                    scene.texture.as_ref().unwrap().clone(),
                    1,
                    Transform::Normal,
                    Some(1.0),
                    None,
                    None,
                    Some(vec![Rectangle::from_size((size.w, size.h).into())]),
                    output.scene_damage.snapshot(),
                    Kind::Unspecified,
                ),
                client: match &scene.frame.storage {
                    SceneStorage::Client(buffer) => Some(buffer.clone()),
                    SceneStorage::Composed(_) => None,
                },
            }
            .into(),
        );
    }
    let modeset = output.compositor.surface().commit_pending();
    let render_started = Instant::now();
    let result = output
        .compositor
        .render_frame(
            renderer,
            &elements,
            Color32F::new(0.0, 0.0, 0.0, 1.0),
            // Smithay tests plane suitability and draws unsupported storage.
            flags,
        )
        .map_err(|error| error.to_string())?;
    let render_us = render_started.elapsed().as_micros() as u64;
    let scene_state = result.states.element_render_state(output.scene_id.clone());
    let cursor_plane = result.cursor_element.is_some();
    let primary_scanout = matches!(result.primary_element, PrimaryPlaneElement::Element(_));
    let primary = match &result.primary_element {
        PrimaryPlaneElement::Swapchain(primary) => Some((
            u32::from(smithay::reexports::drm::buffer::Buffer::handle(
                primary.buffer(),
            )),
            primary.damage.current_commit(),
        )),
        PrimaryPlaneElement::Element(_) => None,
    };
    let primary_reused = primary.is_some() && primary == output.last_primary;
    tracing::trace!(target: "hearthspace::presenter", key = ?output.config.key,
        candidate_submission = output.next_submission + 1,
        scene_sequence, cursor_sequence, cursor_image = ?cursor_image,
        cursor_hotspot = ?snapshot.map(|s| s.hotspot), cursor_plane, primary_scanout, scene_state = ?scene_state, primary_buffer = ?primary.map(|(buffer, _)| buffer),
        primary_damage_commit = ?primary.map(|(_, commit)| commit),
        primary_reused, render_us, flags = ?flags, empty = result.is_empty,
        "rendered native plane selection");

    if let PrimaryPlaneElement::Swapchain(primary) = &result.primary_element {
        if let Some(scene) = output.scene.as_mut() {
            scene.read_sync = Some(primary.sync.clone());
        }
        if result.needs_sync() {
            primary.sync.wait().map_err(|error| error.to_string())?;
        }
    }
    if result.is_empty {
        return Ok(RenderOutcome::Empty);
    }

    if modeset {
        // Virtio can timestamp a modeset event with the preceding vblank, before
        // this output's cutoff. Commit synchronously without an event instead;
        // the scheduler then requires a normal, freshly timestamped page flip.
        // No capture, lock barrier, or client callback is acknowledged here.
        drop(result);
        if !mailbox.permits(control) {
            return Ok(RenderOutcome::Revoked);
        }
        output
            .compositor
            .commit_frame()
            .map_err(|error| match error {
                FrameError::DrmError(error) => {
                    AttemptError::queue(cursor_plane || primary_scanout, error)
                }
                error => AttemptError::from(error.to_string()),
            })?;
        return Ok(RenderOutcome::Primed);
    }

    let capture = output
        .scene
        .as_mut()
        .filter(|scene| scene.frame.capture && !scene.capture_taken)
        .map(|scene| {
            let captured = capture::read_submission(
                renderer,
                &result,
                output.config.size,
                &scene.frame.dmabuf,
                &mut output.capture_buffer,
            );
            // A readback error must not invalidate successful scene rendering.
            // Retain the lease if a partial capture can't be synchronized.
            scene.capture_result(captured, || finish_partial(renderer))
        });
    drop(result);
    if !mailbox.permits(control) {
        // Lifecycle/security may have changed during a driver call. This prepared
        // frame is never submitted; its source read fence is retained above.
        return Ok(RenderOutcome::Revoked);
    }
    output.next_submission += 1;
    let queue_started = Instant::now();
    let sequence = output.next_submission;
    let span = tracing::trace_span!(target: "hearthspace::presenter", "native_queue",
        key = ?output.config.key, submission_sequence = sequence, scene_sequence,
        cursor_sequence, cursor_image = ?cursor_image, cursor_plane, primary_scanout,
        security_epoch = control.security_epoch, lock_generation = ?control.lock_generation,
        primary_buffer = ?primary.map(|(buffer, _)| buffer),
        primary_damage_commit = ?primary.map(|(_, commit)| commit), primary_reused, render_us);
    let _entered = span.enter();
    let submission = Submission {
        sequence,
        scene_sequence,
        cursor_sequence,
        cursor_image,
        security_epoch: control.security_epoch,
        lock_generation: control.lock_generation,
        scene_changed: output.last_presented_scene != Some(scene_sequence),
        capture,
        diagnostics: SubmissionDiagnostics {
            cursor_plane,
            primary_scanout,
            window_id: output
                .scene
                .as_ref()
                .and_then(|scene| scene.frame.window_id)
                .map(std::num::NonZeroU64::get),
            primary,
            primary_reused,
            render_us,
            queue_started,
        },
    };
    let queue_call_started = Instant::now();
    let queued = output.compositor.queue_frame(submission);
    let queue_us = queue_call_started.elapsed().as_micros() as u64;
    tracing::trace!(target: "hearthspace::presenter",
        queue_us,
        accepted = queued.is_ok(), error = ?queued.as_ref().err(),
        "native queue result");
    queued.map_err(|error| match error {
        FrameError::DrmError(error) => AttemptError::queue(cursor_plane || primary_scanout, error),
        error => AttemptError::from(error.to_string()),
    })?;
    output.last_primary = primary;
    if let Some(scene) = output.scene.as_mut() {
        scene.submitted = true;
        scene.capture_taken |= scene.frame.capture;
    }
    Ok(RenderOutcome::Queued)
}
