use super::super::{control_osd, control_state::Device};
use super::*;
use crate::controls::{audio::AudioStatus, brightness::BrightnessStatus};

fn audio(volume: u8) -> Reading {
    Reading::Audio(AudioStatus {
        volume,
        muted: false,
        output: "Speakers".into(),
    })
}

fn request(id: u64, action: Action) -> Request {
    Request {
        id,
        action,
        output: "eDP-1".into(),
        token: None,
        created: Instant::now(),
    }
}

fn hardware_key(id: u64, action: Action) -> Request {
    let mut request = request(id, action);
    request.token = Some((1 << 63) | id);
    request
}

#[test]
fn slider_updates_coalesce_without_losing_relative_keys() {
    let mut queue = VecDeque::new();
    for (id, action) in [
        (1, Action::SetVolume(20)),
        (2, Action::SetVolume(40)),
        (3, Action::ToggleMute),
        (4, Action::SetVolume(60)),
        (5, Action::SetBrightness(80)),
    ] {
        control_worker::coalesce(&mut queue, request(id, action));
    }
    assert_eq!(queue.iter().map(|r| r.id).collect::<Vec<_>>(), [2, 3, 4, 5]);
    let mut key = request(6, Action::VolumeStep(5));
    key.token = Some(99);
    control_worker::coalesce(&mut queue, key.clone());
    control_worker::coalesce(&mut queue, key);
    assert_eq!(queue.len(), 6);
}

#[test]
fn late_readback_preserves_newer_slider_intent_without_opening_an_osd() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(32);
    state.controls.sender = Some(sender);
    submit(&mut state, "eDP-1", Action::SetVolume(40), None);
    let first = receiver.try_recv().unwrap();
    submit(&mut state, "eDP-1", Action::SetVolume(80), None);
    let second = receiver.try_recv().unwrap();
    assert!(state.outputs[0].osd.is_none());
    apply(&mut state, Update::Finished(first, Ok(audio(40)), true));
    assert_eq!(state.controls.volume(), Some(80));
    assert!(state.outputs[0].osd.is_none());
    apply(&mut state, Update::Finished(second, Ok(audio(80)), true));
    assert!(state.controls.volume_pending.is_none());
    assert_eq!(state.controls.volume(), Some(80));
    assert!(state.outputs[0].osd.is_none());
}

#[test]
fn status_dropdown_successes_and_failures_leave_existing_osd_untouched() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    apply(
        &mut state,
        Update::Finished(hardware_key(1, Action::VolumeStep(5)), Ok(audio(40)), true),
    );
    let previous = state.outputs[0].osd.as_ref().unwrap().clone();
    for action in [
        Action::SetVolume(80),
        Action::SetBrightness(70),
        Action::ToggleMute,
    ] {
        let reading = match action {
            Action::SetBrightness(_) => Reading::Brightness(BrightnessStatus {
                percentage: 70,
                device: "panel".into(),
            }),
            Action::ToggleMute => Reading::Audio(AudioStatus {
                volume: 80,
                muted: true,
                output: "Speakers".into(),
            }),
            _ => audio(80),
        };
        for result in [Ok(reading), Err("Unavailable".into())] {
            state.toggle_quick_panel(
                "eDP-1",
                if matches!(action, Action::SetBrightness(_)) {
                    super::super::quick_panel::Kind::Brightness
                } else {
                    super::super::quick_panel::Kind::Sound
                },
            );
            apply(
                &mut state,
                Update::Finished(request(2, action), result, true),
            );
            let current = state.outputs[0].osd.as_ref().unwrap();
            assert_eq!(
                (
                    current.device,
                    current.value,
                    current.muted,
                    &current.error,
                    current.expires
                ),
                (
                    previous.device,
                    previous.value,
                    previous.muted,
                    &previous.error,
                    previous.expires
                ),
                "panel state and result must not change or extend the hardware-key OSD"
            );
        }
    }
    control_osd::expire(&mut state, previous.expires);
    assert!(state.outputs[0].osd.is_none());
    apply(
        &mut state,
        Update::Finished(
            request(3, Action::ToggleMute),
            Err("Unavailable".into()),
            true,
        ),
    );
    assert_eq!(state.controls.volume_error.as_deref(), Some("Unavailable"));
    assert!(state.outputs[0].osd.is_none());
}

#[test]
fn hardware_key_failures_show_feedback_only_after_authorized_completion() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
    state.controls.sender = Some(sender);
    submit(&mut state, "eDP-1", Action::ToggleMute, Some(1 << 63));
    let request = receiver.try_recv().unwrap();
    assert!(state.outputs[0].osd.is_none());
    apply(
        &mut state,
        Update::Finished(request.clone(), Err("Unavailable".into()), false),
    );
    assert!(state.outputs[0].osd.is_none());
    apply(
        &mut state,
        Update::Finished(request, Err("Unavailable".into()), true),
    );
    assert_eq!(
        state.outputs[0].osd.as_ref().unwrap().error.as_deref(),
        Some("Unavailable")
    );
}

#[test]
fn inactivity_clears_osds_and_late_results_do_not_reopen_them() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    apply(
        &mut state,
        Update::Finished(hardware_key(1, Action::VolumeStep(5)), Ok(audio(40)), true),
    );
    assert!(state.outputs[0].osd.is_some());
    apply(&mut state, Update::Poll(Snapshot::default(), false));
    apply(
        &mut state,
        Update::Finished(hardware_key(2, Action::VolumeStep(5)), Ok(audio(80)), false),
    );
    assert!(state.outputs[0].osd.is_none());
    apply(
        &mut state,
        Update::Finished(
            hardware_key(3, Action::ToggleMute),
            Err("Locked".into()),
            false,
        ),
    );
    assert!(state.outputs[0].osd.is_none());
}

#[test]
fn recovered_device_clears_its_unavailable_error_without_showing_an_osd() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.controls.volume_error = Some("No audio output is available.".into());
    let snapshot = Snapshot {
        audio: Ok(AudioStatus {
            volume: 35,
            muted: false,
            output: "Headphones".into(),
        }),
        brightness: Ok(None),
    };
    apply(&mut state, Update::Poll(snapshot, true));
    assert!(state.controls.volume_error.is_none());
    assert_eq!(state.controls.volume(), Some(35));
    assert!(state.outputs[0].osd.is_none());
}

#[test]
fn osd_replaces_previous_device_restarts_timeout_and_preserves_open_panel() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into(), "HDMI-A-1".into()]);
    state.toggle_quick_panel("eDP-1", super::super::quick_panel::Kind::Sound);
    state.controls.snapshot.brightness = Ok(Some(BrightnessStatus {
        percentage: 60,
        device: "panel".into(),
    }));
    apply(
        &mut state,
        Update::Finished(hardware_key(1, Action::VolumeStep(5)), Ok(audio(40)), true),
    );
    let old = state.outputs[0].osd.as_ref().unwrap().expires;
    apply(
        &mut state,
        Update::Finished(
            hardware_key(2, Action::BrightnessStep(5)),
            Ok(Reading::Brightness(BrightnessStatus {
                percentage: 65,
                device: "panel".into(),
            })),
            true,
        ),
    );
    assert!(state.outputs[0].show_controls);
    assert!(state.outputs[0].osd.as_ref().unwrap().expires >= old);
    assert_eq!(
        state.outputs[0].osd.as_ref().unwrap().device,
        Device::Brightness
    );
    let deadline = state.outputs[0].osd.as_ref().unwrap().expires;
    control_osd::expire(&mut state, deadline - Duration::from_millis(1));
    assert!(state.outputs[0].osd.is_some());
    control_osd::expire(&mut state, deadline);
    assert!(state.outputs[0].osd.is_none());
    assert!(state.outputs[0].show_controls);
}
