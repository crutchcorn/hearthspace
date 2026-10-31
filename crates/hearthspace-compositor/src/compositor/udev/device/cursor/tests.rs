//! The real policy owns decisions; only cursor-device operations are substituted.
use smithay::reexports::drm::control::crtc;

use super::policy::{CursorFrame, CursorOps, ImageKey, Scene, Target};
use crate::compositor::cursor::CursorIcon;

mod failures;
mod handoff;
mod independent_clear;
mod lifecycle;
mod retry;

#[derive(Debug, PartialEq)]
pub(in crate::compositor::udev) enum Op {
    ClearAll,
    Image(ImageKey),
    Set(crtc::Handle),
    Move(crtc::Handle, (i32, i32)),
    Clear(crtc::Handle),
}

#[derive(Default)]
pub(in crate::compositor::udev) struct Drm {
    pub calls: Vec<Op>,
    pub image_error: bool,
    pub set_error: bool,
    pub move_error: bool,
    pub clear_error: bool,
    pub clear_errors: std::collections::HashSet<crtc::Handle>,
    pub visible: std::collections::HashSet<crtc::Handle>,
    image: Option<ImageKey>,
    assigned: Option<crtc::Handle>,
}

impl CursorOps for Drm {
    fn clear_all_planes(&mut self) {
        self.calls.push(Op::ClearAll);
        // The legacy clears still cover drivers whose atomic plane clear fails.
    }

    fn image(&mut self, key: ImageKey) -> Option<(i32, i32)> {
        if self.image != Some(key) {
            self.calls.push(Op::Image(key));
            if self.image_error {
                return None;
            }
            self.image = Some(key);
            self.assigned = None;
        }
        Some(if key.icon == CursorIcon::Default {
            (2, 3)
        } else {
            (12, 12)
        })
    }

    fn set(&mut self, crtc: crtc::Handle) -> bool {
        if self.assigned == Some(crtc) {
            return true;
        }
        self.calls.push(Op::Set(crtc));
        if self.set_error {
            return false;
        }
        self.assigned = Some(crtc);
        self.visible.insert(crtc);
        true
    }

    fn move_to(&mut self, crtc: crtc::Handle, position: (i32, i32)) -> bool {
        self.calls.push(Op::Move(crtc, position));
        !self.move_error
    }

    fn clear(&mut self, crtc: crtc::Handle) -> bool {
        self.calls.push(Op::Clear(crtc));
        if self.clear_error || self.clear_errors.contains(&crtc) {
            return false;
        }
        if self.assigned == Some(crtc) {
            self.assigned = None;
        }
        self.visible.remove(&crtc);
        true
    }
}

pub(in crate::compositor::udev) fn target(crtc: u32) -> Target {
    Target {
        crtc: std::num::NonZeroU32::new(crtc).unwrap().into(),
        image: ImageKey {
            icon: CursorIcon::Default,
            scale: 1,
        },
        local: (100.0, 80.0),
    }
}

pub(in crate::compositor::udev) fn scene(frames: &[&CursorFrame], replacement: usize) -> Scene {
    Scene {
        software_displayed: frames.iter().any(|frame| frame.displayed),
        software_may_arrive: frames.iter().any(|frame| frame.may_arrive()),
        replacement_displayed: frames[replacement].displayed,
    }
}
