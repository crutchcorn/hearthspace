//! Legacy cursor ownership follows what KMS has actually presented. The only
//! substituted boundary in the regressions is image upload / DRM cursor ioctls.
use smithay::reexports::drm::control::crtc;

use crate::compositor::cursor::CursorIcon;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::compositor::udev) struct ImageKey {
    pub icon: CursorIcon,
    pub scale: i32,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::compositor::udev) struct Target {
    pub crtc: crtc::Handle,
    pub image: ImageKey,
    pub local: (f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::compositor::udev) struct Placement {
    pub crtc: crtc::Handle,
    pub position: Option<(i32, i32)>,
}

/// Committed pixels are distinct from the latest software-cursor request.
#[derive(Default, Debug)]
pub(in crate::compositor::udev) struct CursorFrame {
    pub displayed: bool,
    pending: Option<bool>,
    uncertain: bool,
}

impl CursorFrame {
    pub fn queued(&mut self, software: bool) {
        self.pending = Some(software);
    }

    pub fn completed(&mut self, verified: bool) {
        if let Some(software) = self.pending.take() {
            if verified {
                self.displayed = software;
                self.uncertain = false;
            } else {
                // An untimed modeset releases GBM, but cannot prove the cursor
                // disappeared or authorize retirement of retained hardware.
                self.uncertain |= software;
            }
        }
    }

    pub fn may_arrive(&self) -> bool {
        self.pending == Some(true) || self.uncertain
    }
}

#[derive(Clone, Copy, Default)]
pub(in crate::compositor::udev) struct Scene {
    pub software_displayed: bool,
    pub software_may_arrive: bool,
    pub replacement_displayed: bool,
}

/// Local external-device boundary; no renderer, protocol or input policy is mocked.
pub(in crate::compositor::udev) trait CursorOps {
    fn clear_all_planes(&mut self);
    fn image(&mut self, key: ImageKey) -> Option<(i32, i32)>;
    fn set(&mut self, crtc: crtc::Handle) -> bool;
    fn move_to(&mut self, crtc: crtc::Handle, position: (i32, i32)) -> bool;
    fn clear(&mut self, crtc: crtc::Handle) -> bool;
}

#[derive(Default)]
pub(in crate::compositor::udev) struct CursorPlane {
    pub active: Option<Placement>,
    hotspot: (i32, i32),
    failed: Option<(crtc::Handle, ImageKey)>,
    initialized: bool,
    retired: Vec<crtc::Handle>,
    clear_failures: Vec<ClearFailure>,
}

const CLEAR_RETRIES: u8 = 4;

struct ClearFailure {
    crtc: crtc::Handle,
    blocked: bool,
    remaining: u8,
}

impl CursorPlane {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn failed(&self, target: Target) -> bool {
        self.failed == Some((target.crtc, target.image))
    }

    pub fn allow_retry(&mut self) {
        self.failed = None;
    }

    pub fn initialize(
        &mut self,
        crtcs: impl Iterator<Item = crtc::Handle>,
        ops: &mut impl CursorOps,
    ) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        ops.clear_all_planes();
        // KMS may retain a cursor from the preceding owner although this fresh
        // policy has no active placement, including on the non-pointer GPU.
        self.retired.extend(crtcs);
        self.clear_retired(ops);
    }

    pub fn clear_retry_pending(&self) -> bool {
        self.clear_failures
            .iter()
            .any(|failure| failure.blocked && failure.remaining > 0)
    }

    /// Only the bounded event-loop timer grants another attempt. Motion and
    /// unrelated completions cannot hammer a failing legacy clear ioctl.
    pub fn retry_clear(&mut self) {
        for failure in &mut self.clear_failures {
            if failure.blocked && failure.remaining > 0 {
                failure.remaining -= 1;
                failure.blocked = false;
            }
        }
    }

    pub fn stop_clear_retries(&mut self) {
        for failure in &mut self.clear_failures {
            failure.remaining = 0;
            failure.blocked = true;
        }
    }

    pub(in crate::compositor::udev) fn clear(&mut self, ops: &mut impl CursorOps) -> bool {
        if let Some(active) = self.active
            && clear_crtc(&mut self.clear_failures, active.crtc, ops)
        {
            self.active = None;
        }
        // One failed CRTC cannot prevent another plane's cleanup. Failed
        // ownership stays tracked even when the other cursor clears promptly.
        self.clear_retired(ops);
        self.active.is_none() && self.retired.is_empty()
    }

    fn clear_retired(&mut self, ops: &mut impl CursorOps) -> bool {
        let failures = &mut self.clear_failures;
        self.retired
            .retain(|crtc| !clear_crtc(failures, *crtc, ops));
        self.retired.is_empty()
    }

    /// Returns whether the next scene must carry software. Retain the old
    /// hardware image until the replacement output verifies its software frame.
    pub(in crate::compositor::udev) fn update(
        &mut self,
        target: Target,
        software: bool,
        scene: Scene,
        ops: &mut impl CursorOps,
    ) -> bool {
        self.clear_retired(ops);
        // A retired target must finish cleanup before hardware can reuse it.
        // Its software replacement can still retire the other active CRTC.
        if software || self.failed(target) || self.retired.contains(&target.crtc) {
            if scene.replacement_displayed {
                self.clear(ops);
            } else if !self.failed(target) {
                self.move_retained(target, ops);
            }
            return true;
        }

        if scene.software_displayed {
            if scene.replacement_displayed {
                self.clear(ops);
            } else {
                // A superseded cursor on another output is not a replacement
                // for the original image under the current pointer.
                self.move_retained(target, ops);
            }
            return false; // submit removal; the displayed software remains meanwhile
        }
        if scene.software_may_arrive {
            self.move_retained(target, ops);
            return false; // an already submitted software frame cannot be cancelled
        }

        let Some(hotspot) = ops.image(target.image) else {
            self.failed = Some((target.crtc, target.image));
            return true;
        };
        let position = position(target, hotspot);
        let previous = self.active;
        // The ioctl may be required even for an unchanged CRTC after an image
        // upload. CursorOps caches the successfully assigned image separately.
        if !ops.set(target.crtc) {
            self.failed = Some((target.crtc, target.image));
            return true;
        }
        let placed = Placement {
            crtc: target.crtc,
            position: Some(position),
        };
        if self.active != Some(placed) {
            if !ops.move_to(target.crtc, position) {
                self.failed = Some((target.crtc, target.image));
                if previous.is_some_and(|active| active.crtc != target.crtc) {
                    // The new plane may be visible at an unknown position. Keep
                    // the original image on A and retire only the failed B.
                    self.retired.push(target.crtc);
                    self.clear_retired(ops);
                    return true;
                }
                // A successful set is visible even if its move failed. Retain
                // ownership so the software completion will clear that plane.
                self.active.get_or_insert(Placement {
                    crtc: target.crtc,
                    position: None,
                });
                return true;
            }
            self.active = Some(placed);
        }
        self.hotspot = hotspot;
        // Re-adopting current hardware supersedes its earlier clear request.
        // Forget only this CRTC's obsolete retry, never another failed plane.
        self.clear_failures
            .retain(|failure| failure.crtc != target.crtc);
        if let Some(previous) = previous.filter(|active| active.crtc != target.crtc) {
            // Legacy operations cannot be atomic. Establish B before clearing
            // A so failed assignment never removes the original cursor.
            self.retired.push(previous.crtc);
            self.clear_retired(ops);
        }
        self.failed = None;
        false
    }

    pub(in crate::compositor::udev) fn retire_previous(
        &mut self,
        scene: Scene,
        replacement_hardware: bool,
        ops: &mut impl CursorOps,
    ) {
        self.clear_retired(ops);
        // Leaving this device is an explicit ownership change. Returning may
        // retry its plane; steady motion on a failing device still never does.
        self.allow_retry();
        if scene.replacement_displayed || replacement_hardware {
            self.clear(ops);
        }
    }

    fn move_retained(&mut self, target: Target, ops: &mut impl CursorOps) {
        if let Some(active) = self.active.as_mut()
            && active.crtc == target.crtc
        {
            let position = position(target, self.hotspot);
            if Some(position) != active.position && ops.move_to(active.crtc, position) {
                active.position = Some(position);
            }
        }
    }
}

fn clear_crtc(
    failures: &mut Vec<ClearFailure>,
    crtc: crtc::Handle,
    ops: &mut impl CursorOps,
) -> bool {
    let previous = failures.iter().position(|failure| failure.crtc == crtc);
    if previous.is_some_and(|index| failures[index].blocked) {
        return false;
    }
    if ops.clear(crtc) {
        if let Some(index) = previous {
            failures.swap_remove(index);
        }
        return true;
    }
    let index = previous.unwrap_or_else(|| {
        failures.push(ClearFailure {
            crtc,
            blocked: false,
            remaining: CLEAR_RETRIES,
        });
        failures.len() - 1
    });
    let failure = &mut failures[index];
    failure.blocked = true;
    if failure.remaining == 0 {
        tracing::warn!(
            ?crtc,
            "native cursor clear exhausted bounded retries; retaining ownership until lifecycle recovery"
        );
    }
    false
}

fn position(target: Target, hotspot: (i32, i32)) -> (i32, i32) {
    (
        target.local.0.round() as i32 - hotspot.0,
        target.local.1.round() as i32 - hotspot.1,
    )
}
