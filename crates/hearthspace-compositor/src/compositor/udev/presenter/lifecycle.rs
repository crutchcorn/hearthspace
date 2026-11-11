use super::{
    PresenterEvent, cursor::CursorCache, mailbox::Control, output::Output, render, worker::Worker,
};

impl Worker {
    pub(super) fn apply_security(&mut self, control: &Control) {
        let security = (control.security_epoch, control.lock_generation);
        if self.security == security {
            return;
        }
        self.security = security;
        self.cursor = None;
        let events = self.events.clone();
        for output in self.outputs.values_mut() {
            for scene in [output.scene.take(), output.desired.take()]
                .into_iter()
                .flatten()
            {
                self.retirement.push(scene, |event| {
                    let _ = events.send(event);
                });
            }
            output.cursor = None;
            output.cursor_cache = CursorCache::default();
            output
                .presented
                .security_changed(control.lock_generation.is_some());
            output.schedule.invalidate_security();
            // A pending old frame still needs its real vblank to release
            // Smithay's state. Its security tag prevents acknowledging it.
            // New locks render opaque black immediately. An already verified
            // lock may remain visible until its fresh scene arrives, but never
            // acknowledges the new generation without a physical submission.
        }
    }

    pub(super) fn reconcile(&mut self, control: &Control) -> bool {
        let changing =
            control.stopped || !control.active || self.configured != control.configuration;
        if !changing {
            return true;
        }
        if !control.active || control.stopped {
            // pause marks shared DrmSurface state inactive before any drops,
            // avoiding cleanup ioctls on a seat descriptor already revoked.
            if self.drm.is_active() {
                self.drm.pause();
            }
        }
        if !self.outputs.is_empty() {
            if self.drm.is_active() {
                for output in self.outputs.values_mut() {
                    if let Err(error) = output.compositor.clear() {
                        let _ = self.events.send(PresenterEvent::Failed {
                            output: Some(output.config.key),
                            error: error.to_string(),
                        });
                        // A failed clear must not leave the new compositor
                        // trusting old plane state. Activation below resets it.
                        self.drm.pause();
                        break;
                    }
                }
            }
            let events = self.events.clone();
            for (_, mut output) in self.outputs.drain() {
                for mut scene in [output.scene.take(), output.desired.take()]
                    .into_iter()
                    .flatten()
                {
                    // Reconfiguration may cancel an already queued capture.
                    // Main keys cancellation by output generation + scene seq.
                    scene.submitted = false;
                    self.retirement.push(scene, |event| {
                        let _ = events.send(event);
                    });
                }
            }
            if !control.active || control.stopped {
                self.cursor = None;
            }
        }
        if self.retirement.has_uncertain() && render::finish_partial(&mut self.renderer) {
            self.retirement.resolve_uncertain();
        }
        let events = self.events.clone();
        self.retirement.poll(|event| {
            let _ = events.send(event);
        });
        if !self.retirement.is_empty() {
            return false;
        }

        // Never acknowledge an obsolete operation after fence retirement.
        let latest = self.mailbox.current();
        if latest.generation != control.generation
            || latest.stopped != control.stopped
            || latest.security_epoch != control.security_epoch
            || latest.lock_generation != control.lock_generation
        {
            return false;
        }
        if control.stopped {
            self.done = true;
            return false;
        }
        if !control.active {
            if self.paused_ack != Some(control.generation) {
                self.paused_ack = Some(control.generation);
                self.emit(PresenterEvent::Paused {
                    control_generation: control.generation,
                });
            }
            return false;
        }
        if !self.mailbox.permits(control) {
            return false;
        }
        if !self.drm.is_active()
            && let Err(error) = self.drm.activate(true)
        {
            self.emit(PresenterEvent::Failed {
                output: None,
                error: error.to_string(),
            });
            self.mailbox.stop();
            return false;
        }
        self.paused_ack = None;
        for config in &control.outputs {
            if config.key.device != self.device {
                self.emit(PresenterEvent::Failed {
                    output: Some(config.key),
                    error: "configuration belongs to another DRM device".into(),
                });
                continue;
            }
            if self
                .outputs
                .values()
                .any(|output| output.config.key.crtc == config.key.crtc)
            {
                self.emit(PresenterEvent::Failed {
                    output: Some(config.key),
                    error: "configuration assigns a CRTC twice".into(),
                });
                continue;
            }
            match Output::new(&mut self.drm, &self.gbm, &self.renderer, config.clone()) {
                Ok(mut output) => {
                    output.cursor = self.cursor.clone();
                    self.outputs.insert(config.key, output);
                }
                Err(error) => self.emit(PresenterEvent::Failed {
                    output: Some(config.key),
                    error,
                }),
            }
            if !self.mailbox.permits(control) {
                return false;
            }
        }
        // Every prior generation's leases are gone before any new pool is
        // authorized. All configuration ioctls have finished before publication.
        if self.mailbox.permits(control) {
            self.configured = control.configuration;
            for output in self.outputs.values() {
                self.emit(PresenterEvent::Configured {
                    key: output.config.key,
                    control_generation: control.generation,
                    format: output.compositor.format(),
                    modifiers: output.compositor.modifiers().to_vec(),
                });
            }
            true
        } else {
            false
        }
    }
}
