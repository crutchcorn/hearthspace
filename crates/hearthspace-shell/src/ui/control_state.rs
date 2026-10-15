//! Shared readings and short-lived, coalescible adjustment requests.
use std::time::Instant;

use crate::controls::{audio::AudioStatus, brightness::BrightnessStatus};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Device {
    Volume,
    Brightness,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    SetVolume(u8),
    SetBrightness(u8),
    VolumeStep(i8),
    BrightnessStep(i8),
    ToggleMute,
}

impl Action {
    pub fn device(self) -> Device {
        match self {
            Self::SetBrightness(_) | Self::BrightnessStep(_) => Device::Brightness,
            _ => Device::Volume,
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        Some(match value {
            "volume-up" => Self::VolumeStep(5),
            "volume-down" => Self::VolumeStep(-5),
            "volume-mute" => Self::ToggleMute,
            "brightness-up" => Self::BrightnessStep(5),
            "brightness-down" => Self::BrightnessStep(-5),
            _ => return None,
        })
    }

    pub fn absolute(self) -> Option<u8> {
        match self {
            Self::SetVolume(value) | Self::SetBrightness(value) => Some(value),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Request {
    pub id: u64,
    pub output: String,
    pub action: Action,
    pub token: Option<u64>,
    pub created: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub audio: Result<AudioStatus, String>,
    pub brightness: Result<Option<BrightnessStatus>, String>,
}

#[derive(Clone, Debug)]
pub(super) enum Reading {
    Audio(AudioStatus),
    Brightness(BrightnessStatus),
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            audio: Err("Checking audio…".into()),
            brightness: Err("Checking display brightness…".into()),
        }
    }
}

#[derive(Default)]
pub(super) struct Controls {
    pub snapshot: Snapshot,
    pub sender: Option<tokio::sync::mpsc::Sender<Request>>,
    pub volume_pending: Option<(u64, u8)>,
    pub brightness_pending: Option<(u64, u8)>,
    pub volume_error: Option<String>,
    pub brightness_error: Option<String>,
    pub next_id: u64,
}

impl Controls {
    pub fn volume(&self) -> Option<u8> {
        self.volume_pending
            .map(|(_, value)| value)
            .or_else(|| self.snapshot.audio.as_ref().ok().map(|audio| audio.volume))
    }

    pub fn brightness(&self) -> Option<u8> {
        self.brightness_pending.map(|(_, value)| value).or_else(|| {
            self.snapshot
                .brightness
                .as_ref()
                .ok()
                .and_then(|reading| reading.as_ref().map(|reading| reading.percentage))
        })
    }

    pub fn error_mut(&mut self, device: Device) -> &mut Option<String> {
        match device {
            Device::Volume => &mut self.volume_error,
            Device::Brightness => &mut self.brightness_error,
        }
    }

    pub fn pending_mut(&mut self, device: Device) -> &mut Option<(u64, u8)> {
        match device {
            Device::Volume => &mut self.volume_pending,
            Device::Brightness => &mut self.brightness_pending,
        }
    }
}
