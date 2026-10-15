//! Hardware I/O and compositor authorization never run on the UI thread.
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use super::{
    command_client,
    control_state::{Action, Reading, Request, Snapshot, stepped},
};
use crate::controls::{audio::AudioBackend, brightness::Brightness};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Session {
    pub native: bool,
    pub available: bool,
}

pub(super) async fn session(socket: PathBuf) -> Session {
    tokio::task::spawn_blocking(move || {
        let payload = command_client::query_canvas(&socket).ok()?;
        let value: serde_json::Value = serde_json::from_slice(&payload).ok()?;
        if value["version"] != 1
            || !matches!(
                value["backend"].as_str(),
                Some("native" | "nested" | "headless")
            )
        {
            return None;
        }
        Some(Session {
            native: value["backend"] == "native",
            available: value["active"] == true
                && value["locked"] == false
                && value["recovery_ready"] == true,
        })
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default()
}

pub(super) async fn snapshot(native: bool) -> Snapshot {
    let audio = AudioBackend::default();
    let brightness = Brightness::new(native);
    let (audio, brightness) = tokio::join!(audio.status(), brightness.read());
    Snapshot { audio, brightness }
}

async fn accept(socket: &Path, request: &Request) -> Result<(), String> {
    if request.created.elapsed() > Duration::from_secs(2) {
        return Err("The adjustment expired. Try again.".into());
    }
    if !session(socket.to_path_buf()).await.available {
        return Err("Controls are unavailable while this desktop is inactive or locked.".into());
    }
    if let Some(token) = request.token {
        let socket = socket.to_path_buf();
        tokio::task::spawn_blocking(move || {
            command_client::send_command(&socket, &format!("shell-event-accept {token}"))
        })
        .await
        .map_err(|_| "The keyboard adjustment could not be confirmed.".to_string())??;
    }
    if request.created.elapsed() > Duration::from_secs(2) {
        return Err("The adjustment expired. Try again.".into());
    }
    Ok(())
}

pub(super) async fn adjust(socket: &Path, request: &Request) -> (Result<Reading, String>, bool) {
    let accepted = AtomicBool::new(false);
    let result = tokio::time::timeout(
        Duration::from_secs(4),
        adjust_inner(socket, request, &accepted),
    )
    .await
    .unwrap_or_else(|_| {
        Err(
            "The adjustment could not be confirmed. Check the current level before trying again."
                .into(),
        )
    });
    // Unavailable devices deserve feedback only for a still-current request.
    if result.is_err() && !accepted.load(Ordering::Relaxed) && accept(socket, request).await.is_ok()
    {
        accepted.store(true, Ordering::Relaxed);
    }
    (result, accepted.load(Ordering::Relaxed))
}

async fn adjust_inner(
    socket: &Path,
    request: &Request,
    accepted: &AtomicBool,
) -> Result<Reading, String> {
    if request.created.elapsed() > Duration::from_secs(2) {
        return Err("The adjustment expired. Try again.".into());
    }
    let session = session(socket.to_path_buf()).await;
    if !session.available {
        return Err("Controls are unavailable while this desktop is inactive or locked.".into());
    }
    let audio = AudioBackend::default();
    let brightness = Brightness::new(session.native);
    // Resolve relative changes from a fresh reading, never an optimistic UI value.
    let action = match request.action {
        Action::VolumeStep(step) => {
            Action::SetVolume(stepped(audio.status().await?.volume, step, 0))
        }
        Action::BrightnessStep(step) => {
            let current = brightness
                .read()
                .await?
                .ok_or_else(|| "This display has no adjustable backlight.".to_string())?;
            Action::SetBrightness(stepped(current.percentage, step, 1))
        }
        action => action,
    };
    let mut token = request.token;
    let guard = || {
        let mut authorization = request.clone();
        authorization.token = token.take();
        async move {
            accept(socket, &authorization).await?;
            accepted.store(true, Ordering::Relaxed);
            Ok(())
        }
    };
    match action {
        Action::SetVolume(value) => {
            let actual = audio
                .set_volume_unmuted_guarded(value.min(100), guard)
                .await?;
            Ok(Reading::Audio(actual))
        }
        Action::ToggleMute => {
            let actual = audio.toggle_mute_guarded(guard).await?;
            Ok(Reading::Audio(actual))
        }
        Action::SetBrightness(value) => {
            let actual = brightness.set_guarded(value.clamp(1, 100), guard).await?;
            Ok(Reading::Brightness(actual))
        }
        Action::VolumeStep(_) | Action::BrightnessStep(_) => unreachable!(),
    }
}

/// Only supersede consecutive absolute slider values for the same device.
/// Relative keys and mute toggles retain their order and authorization tokens.
pub(super) fn coalesce(queue: &mut std::collections::VecDeque<Request>, request: Request) {
    if let Some(previous) = queue.back_mut()
        && previous.token.is_none()
        && request.token.is_none()
        && previous.action.absolute().is_some()
        && request.action.absolute().is_some()
        && previous.action.device() == request.action.device()
    {
        *previous = request;
    } else {
        queue.push_back(request);
    }
}

#[cfg(test)]
#[path = "control_worker_tests.rs"]
mod tests;
