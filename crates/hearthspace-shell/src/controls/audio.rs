//! Default output controls for PulseAudio and PipeWire's PulseAudio service.
//!
//! Commands inherit the supervisor's audio routing, including `PULSE_SERVER`.
//! Each request reconnects, so restarting the service or changing the default
//! output does not leave the controls attached to a stale device.
use std::{future::Future, path::PathBuf, time::Duration};

mod model;
mod transport;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod transport_tests;
#[cfg(test)]
mod volume_tests;

/// The current default output, suitable for a 0–100% volume control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioStatus {
    pub volume: u8,
    pub muted: bool,
    pub output: String,
}

/// Bounded, asynchronous access to the user's existing audio service.
#[derive(Clone, Debug)]
pub struct AudioBackend {
    program: PathBuf,
    timeout: Duration,
}

enum Change {
    Volume(u8),
    Mute(&'static str),
}

impl Default for AudioBackend {
    fn default() -> Self {
        Self {
            program: "pactl".into(),
            timeout: Duration::from_millis(1500),
        }
    }
}

impl AudioBackend {
    /// Read the default output again, rather than retaining a device index.
    pub async fn status(&self) -> Result<AudioStatus, String> {
        self.output().await.map(|output| output.status)
    }

    async fn output(&self) -> Result<model::Output, String> {
        let (info, sinks) = tokio::try_join!(
            self.run(&["--format=json", "info"]),
            self.run(&["--format=json", "list", "sinks"]),
        )?;
        model::parse_output(&info, &sinks)
    }

    /// Set a percentage without amplification. Mute remains a separate control.
    pub async fn set_volume(&self, volume: u8) -> Result<AudioStatus, String> {
        self.change_guarded(&[Change::Volume(volume)], || async { Ok(()) })
            .await
    }

    /// Slider and volume-key adjustments can unmute the same selected output.
    pub async fn set_volume_unmuted(&self, volume: u8) -> Result<AudioStatus, String> {
        self.set_volume_unmuted_guarded(volume, || async { Ok(()) })
            .await
    }

    /// Revalidate session permission after discovery and before each write.
    pub async fn set_volume_unmuted_guarded<F>(
        &self,
        volume: u8,
        guard: impl FnMut() -> F,
    ) -> Result<AudioStatus, String>
    where
        F: Future<Output = Result<(), String>>,
    {
        let before = self.output().await?;
        self.volume_output_guarded(before, volume, guard).await
    }

    /// Calculate a key step and apply it to the same discovered output. A newly
    /// selected default must never receive a level derived from its predecessor.
    pub async fn step_volume_unmuted_guarded<F>(
        &self,
        step: i8,
        guard: impl FnMut() -> F,
    ) -> Result<AudioStatus, String>
    where
        F: Future<Output = Result<(), String>>,
    {
        let before = self.output().await?;
        let volume = (i16::from(before.status.volume) + i16::from(step)).clamp(0, 100) as u8;
        self.volume_output_guarded(before, volume, guard).await
    }

    async fn volume_output_guarded<F>(
        &self,
        before: model::Output,
        volume: u8,
        guard: impl FnMut() -> F,
    ) -> Result<AudioStatus, String>
    where
        F: Future<Output = Result<(), String>>,
    {
        if volume > 0 {
            self.change_output_guarded(before, &[Change::Volume(volume), Change::Mute("0")], guard)
                .await
        } else {
            self.change_output_guarded(before, &[Change::Volume(volume)], guard)
                .await
        }
    }

    pub async fn toggle_mute(&self) -> Result<AudioStatus, String> {
        self.toggle_mute_guarded(|| async { Ok(()) }).await
    }

    pub async fn toggle_mute_guarded<F>(
        &self,
        guard: impl FnMut() -> F,
    ) -> Result<AudioStatus, String>
    where
        F: Future<Output = Result<(), String>>,
    {
        self.change_guarded(&[Change::Mute("toggle")], guard).await
    }

    pub async fn set_muted(&self, muted: bool) -> Result<AudioStatus, String> {
        self.change_guarded(&[Change::Mute(if muted { "1" } else { "0" })], || async {
            Ok(())
        })
        .await
    }

    async fn change_guarded<F>(
        &self,
        changes: &[Change],
        guard: impl FnMut() -> F,
    ) -> Result<AudioStatus, String>
    where
        F: Future<Output = Result<(), String>>,
    {
        let before = self.output().await?;
        self.change_output_guarded(before, changes, guard).await
    }

    async fn change_output_guarded<F>(
        &self,
        before: model::Output,
        changes: &[Change],
        mut guard: impl FnMut() -> F,
    ) -> Result<AudioStatus, String>
    where
        F: Future<Output = Result<(), String>>,
    {
        // Pin this request to its freshly resolved device. End option parsing
        // before using a server-provided name as an ordinary argument.
        for change in changes {
            let (command, values) = match change {
                Change::Volume(value) => ("set-sink-volume", before.scaled_volumes(*value)),
                Change::Mute(value) => ("set-sink-mute", vec![value.to_string()]),
            };
            let mut args = vec!["--", command, &before.name];
            args.extend(values.iter().map(String::as_str));
            guard().await?;
            self.run(&args).await?;
        }
        let after = self.output().await?;
        if after.name != before.name {
            return Err("The audio output changed. Try again with the current output.".into());
        }
        Ok(after.status)
    }
}
