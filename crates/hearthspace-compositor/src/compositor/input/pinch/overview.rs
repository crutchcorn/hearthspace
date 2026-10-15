//! An overview pinch is committed only after a successful, uncancelled end.
use smithay::backend::input::InputTime;
use tracing::debug;

use super::{App, PinchInput};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum OverviewAction {
    Open,
    Close,
}

pub(super) struct OverviewGesture {
    input: PinchInput,
    device: String,
    scale: f64,
}

impl OverviewGesture {
    pub(super) fn begin(input: PinchInput, device: String, fingers: u32) -> Option<Self> {
        (matches!(input, PinchInput::Touchpad | PinchInput::Touchscreen)
            && matches!(fingers, 4 | 5))
        .then_some(Self {
            input,
            device,
            scale: 1.0,
        })
    }

    fn owns(&self, input: PinchInput, device: &str) -> bool {
        self.input == input && self.device == device
    }

    fn update(&mut self, input: PinchInput, device: &str, scale: f64) {
        if self.owns(input, device) && scale.is_finite() && scale > 0.0 {
            self.scale = scale;
        }
    }

    fn action(self, cancelled: bool) -> Option<OverviewAction> {
        if cancelled {
            None
        } else if self.scale <= 0.8 {
            Some(OverviewAction::Open)
        } else if self.scale >= 1.25 {
            Some(OverviewAction::Close)
        } else {
            None
        }
    }
}

impl App {
    pub(in crate::compositor::input) fn start_overview_pinch(
        &mut self,
        input: PinchInput,
        device: String,
        fingers: u32,
    ) {
        if self.pinch_zoom.is_active() || self.pinch_blocked() {
            return;
        }
        self.pinch_zoom.overview = OverviewGesture::begin(input, device, fingers);
        if let Some(gesture) = &self.pinch_zoom.overview {
            debug!(device = %gesture.device, fingers, "starting overview pinch gesture");
            self.apply_pointer_motion(self.pointer_location, InputTime::now());
        }
    }

    pub(in crate::compositor::input) fn update_overview_pinch(
        &mut self,
        input: PinchInput,
        device: &str,
        scale: f64,
    ) -> bool {
        let Some(gesture) = self.pinch_zoom.overview.as_mut() else {
            return false;
        };
        gesture.update(input, device, scale);
        if self.pinch_blocked() {
            self.interrupt_pinch_zoom();
        }
        true
    }

    pub(in crate::compositor::input) fn end_overview_pinch(
        &mut self,
        input: PinchInput,
        device: &str,
        cancelled: bool,
    ) -> bool {
        let Some(gesture) = self.pinch_zoom.overview.as_ref() else {
            return false;
        };
        if !gesture.owns(input, device) {
            return true;
        }
        let gesture = self.pinch_zoom.overview.take().unwrap();
        let action = gesture.action(cancelled || self.pinch_blocked());
        debug!(%device, cancelled, ?action, "ending overview pinch gesture");
        match action {
            Some(OverviewAction::Open) => {
                let _ = self.open_overview();
            }
            Some(OverviewAction::Close) => self.close_overview(),
            None => {}
        }
        true
    }
}

#[cfg(test)]
mod tests;
