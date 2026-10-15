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
        let volume = format!("{}%", volume.min(100));
        self.change(&[("set-sink-volume", &volume)]).await
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
        let value = format!("{}%", volume.min(100));
        if volume > 0 {
            self.change_guarded(
                &[("set-sink-volume", &value), ("set-sink-mute", "0")],
                guard,
            )
            .await
        } else {
            self.change_guarded(&[("set-sink-volume", &value)], guard)
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
        self.change_guarded(&[("set-sink-mute", "toggle")], guard)
            .await
    }

    pub async fn set_muted(&self, muted: bool) -> Result<AudioStatus, String> {
        self.change(&[("set-sink-mute", if muted { "1" } else { "0" })])
            .await
    }

    async fn change(&self, changes: &[(&str, &str)]) -> Result<AudioStatus, String> {
        self.change_guarded(changes, || async { Ok(()) }).await
    }

    async fn change_guarded<F>(
        &self,
        changes: &[(&str, &str)],
        mut guard: impl FnMut() -> F,
    ) -> Result<AudioStatus, String>
    where
        F: Future<Output = Result<(), String>>,
    {
        let before = self.output().await?;
        // Pin this request to its freshly resolved device. End option parsing
        // before using a server-provided name as an ordinary argument.
        for &(command, value) in changes {
            guard().await?;
            self.run(&["--", command, &before.name, value]).await?;
        }
        let after = self.output().await?;
        if after.name != before.name {
            return Err("The audio output changed. Try again with the current output.".into());
        }
        Ok(after.status)
    }
}
