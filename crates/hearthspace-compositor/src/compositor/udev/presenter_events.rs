//! Worker notifications carry output generations and exact submission identity.
use super::{
    UdevBackendState, apply_connector_sync,
    identity::DeviceToken,
    presenter::{OutputKey, PresenterEvent},
    udev_backend_mut,
};
use crate::compositor::CalloopData;
use smithay::{
    backend::drm::DrmEventTime,
    reexports::calloop::{
        LoopHandle, RegistrationToken,
        channel::{Channel, Event},
    },
    utils::{Clock, Monotonic},
};
use tracing::{error, trace, warn};

pub(super) fn insert_presenter_source(
    handle: &LoopHandle<'static, CalloopData>,
    token: DeviceToken,
    events: Channel<PresenterEvent>,
) -> Result<RegistrationToken, Box<dyn std::error::Error>> {
    Ok(
        handle.insert_source(events, move |event, _, data| match event {
            Event::Msg(event) => handle_event(data, token, event),
            Event::Closed => failed(data, token, None, "presenter channel closed"),
        })?,
    )
}

fn handle_event(data: &mut CalloopData, token: DeviceToken, event: PresenterEvent) {
    let Some(backend) = udev_backend_mut(data) else {
        return;
    };
    if backend
        .opened_devices
        .get(&token.id)
        .is_none_or(|device| device.token != token)
    {
        return;
    }
    match event {
        PresenterEvent::Ready => {}
        PresenterEvent::Configured {
            key,
            control_generation,
            format,
            modifiers,
        } => {
            let result = backend
                .opened_devices
                .get_mut(&token.id)
                .unwrap()
                .configured(key, control_generation, format, modifiers);
            match result {
                Ok(true) => {
                    backend.refresh_backend_state();
                    let sync = backend.connector_sync();
                    apply_connector_sync(data, sync);
                    data.state.request_redraw();
                }
                Ok(false) => {}
                Err(error) => failed(data, token, Some(key), &error.to_string()),
            }
        }
        PresenterEvent::Presented {
            key,
            submission_sequence,
            scene_sequence,
            cursor_sequence,
            cursor_image,
            security_epoch,
            lock_generation,
            metadata,
            scene_changed,
            planes,
            capture,
        } => {
            let crate::compositor::Backend::Udev(backend) = &mut data.backend else {
                return;
            };
            if !backend.accepts_event(key, security_epoch)
                || data.state.lock_frame_generation() != backend.lock_generation
                || lock_generation != backend.lock_generation
            {
                return;
            }
            let output = backend
                .opened_devices
                .get_mut(&token.id)
                .unwrap()
                .output_surfaces
                .iter_mut()
                .find(|output| output.key == key)
                .unwrap();
            let Some(scene_completed) = output.presentation.presented_with_planes(
                key,
                submission_sequence,
                scene_sequence,
                planes,
            ) else {
                return;
            };
            let name = output.target.connector_name().to_owned();
            if data.state.session_locked() {
                data.captures.invalidate("session is locked");
                data.state.remote_capture.stop();
            } else if let Some(capture) = capture {
                // A different scene's cursor flip must never consume this metadata.
                if output
                    .capture
                    .as_ref()
                    .is_some_and(|(sequence, _, _)| *sequence == scene_sequence)
                {
                    let (_, metadata, requested_before) = output.capture.take().unwrap();
                    match capture {
                        Ok(frame) => {
                            data.state.remote_capture.stage_before(
                                &name,
                                frame.size,
                                frame.clean_pixels,
                                Some(frame.pixels.clone()),
                                requested_before,
                            );
                            if data.captures.wanted(&name)
                                && let Err(error) = data.captures.stage_pixels_before(
                                    metadata,
                                    frame.size,
                                    frame.pixels,
                                    "submitted-planes",
                                    requested_before,
                                )
                            {
                                data.captures
                                    .fail(&name, &format!("native encoding failed: {error}"));
                            }
                        }
                        Err(error) => {
                            data.captures
                                .fail(&name, &format!("native readback failed: {error}"));
                            data.state.remote_capture.fail_output(&name);
                        }
                    }
                }
            }
            // Only staging above can create an unacknowledged frame; it happens
            // in this same callback for the exact verified submission.
            if !data.state.session_locked() {
                data.captures.presented(&name, "page-flip");
                let timestamp = match metadata.time {
                    DrmEventTime::Monotonic(time) => time,
                    _ => Clock::<Monotonic>::new().now().into(),
                };
                data.state.remote_capture.presented(&name, timestamp);
            }
            if let Some(generation) = lock_generation {
                data.state.lock_frame_presented(&name, generation);
            }
            backend.cursor_publication.callbacks(
                &data.state,
                key,
                &cursor_image,
                data.start_time.elapsed().as_millis() as u32,
            );
            trace!(device_id = token.id, incarnation = token.incarnation, crtc = ?key.crtc,
                output = name, submission_sequence, scene_sequence, cursor_sequence, scene_changed,
                "verified native output presentation");
            if scene_completed {
                if let Err(error) = data.send_output_frame_callbacks(&name) {
                    warn!(%error, "failed to send native frame callbacks");
                }
                if animating(data) {
                    data.state.request_redraw();
                }
            } else if let Err(error) = data.display.flush_clients() {
                warn!(%error, "failed to flush cursor callbacks");
            }
        }
        PresenterEvent::FrameOpportunity {
            key,
            security_epoch,
            cursor_image,
        } => {
            let crate::compositor::Backend::Udev(backend) = &mut data.backend else {
                return;
            };
            if backend.accepts_event(key, security_epoch)
                && data.state.lock_frame_generation() == backend.lock_generation
            {
                backend.cursor_publication.callbacks(
                    &data.state,
                    key,
                    &cursor_image,
                    data.start_time.elapsed().as_millis() as u32,
                );
                if let Err(error) = data.display.flush_clients() {
                    warn!(%error, "failed to flush paced cursor callbacks");
                }
            }
        }
        PresenterEvent::SceneDiscarded {
            key,
            scene_sequence,
        } => {
            let Some(output) = backend
                .opened_devices
                .get_mut(&token.id)
                .unwrap()
                .output_surfaces
                .iter_mut()
                .find(|o| o.key == key)
            else {
                return;
            };
            if output.presentation.discarded(key, scene_sequence) {
                let name = output.target.connector_name().to_owned();
                output.capture = None;
                data.captures
                    .fail(&name, "captured native scene was superseded");
                data.state.remote_capture.fail_output(&name);
            }
        }
        // Dispatch itself wakes runtime, which retries only dirty, available
        // outputs. It must not turn another output's pending scene dirty.
        PresenterEvent::SceneCapacityAvailable { key } => {
            trace!(?key, "native scene capacity available");
        }
        PresenterEvent::Paused { control_generation } => {
            let device = backend.opened_devices.get(&token.id).unwrap();
            trace!(
                device_id = token.id,
                control_generation,
                current = device.control_generation,
                "native presenter quiesced"
            );
        }
        PresenterEvent::Failed { output, error } => failed(data, token, output, &error),
        PresenterEvent::Stopped => failed(data, token, None, "native presenter stopped"),
    }
}

impl UdevBackendState {
    fn accepts_event(&self, key: OutputKey, epoch: u64) -> bool {
        self.session_active
            && epoch == self.security_epoch
            && self
                .opened_devices
                .get(&key.device.id)
                .is_some_and(|device| {
                    device.token == key.device
                        && device.active
                        && device
                            .output_surfaces
                            .iter()
                            .any(|output| output.key == key)
                })
    }
}

fn failed(data: &mut CalloopData, token: DeviceToken, output: Option<OutputKey>, error: &str) {
    error!(
        device_id = token.id,
        incarnation = token.incarnation,
        ?output,
        error,
        "native presenter failed"
    );
    let Some(backend) = udev_backend_mut(data) else {
        return;
    };
    let Some(device) = backend
        .opened_devices
        .get_mut(&token.id)
        .filter(|d| d.token == token)
    else {
        return;
    };
    if output.is_some_and(|key| key.generation != device.generation) {
        return;
    }
    if output.is_none() {
        backend.close_device(token.id);
    } else {
        device.configuration_pending = false;
        device.recovery_needed = true;
        device.mark_all_surfaces_dirty();
    }
    backend.refresh_backend_state();
    backend.schedule_recovery_timer();
    let sync = backend.connector_sync();
    apply_connector_sync(data, sync);
    data.state.invalidate_lock_presentation();
}

fn animating(data: &CalloopData) -> bool {
    data.state.viewport_animation.is_some()
        || data.state.group_animating()
        || data.state.canvas_preview_needs_animation_frame()
        || data.state.canvas_peek_needs_animation_frame()
        || data.state.layout_needs_animation_frame()
        || data.state.desktop_edge_panning()
}
