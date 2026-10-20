//! Sound keeps the real slider and mute behavior from the former combined panel.
use super::*;
use crate::{
    controls::audio::AudioStatus,
    ui::test_support::{Ui, ids},
};
use masonry::{
    accesskit::{Action as AccessAction, ActionData, ActionRequest, Role, TreeId},
    core::{
        TextEvent, WidgetId,
        keyboard::{Key, NamedKey},
    },
};

fn view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(panel(state, &state.outputs[0]))
}

fn slider(ui: &Ui<impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox>>) -> WidgetId {
    ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.role() == Role::Slider && node.label().as_deref() == Some("Volume")
            })
        })
        .expect("named volume slider")
}

fn text(ui: &Ui<impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox>>) -> Vec<String> {
    ids(&ui.harness)
        .into_iter()
        .filter_map(|id| {
            ui.harness
                .access_node(id)
                .and_then(|node| node.label().or_else(|| node.value()))
        })
        .collect()
}

#[test]
fn sound_slider_accepts_keyboard_and_accessibility_changes_and_mute_is_separate() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.toggle_quick_panel("eDP-1", Kind::Sound);
    state.controls.snapshot.audio = Ok(AudioStatus {
        volume: 40,
        muted: false,
        output: "Speakers".into(),
    });
    let (sender, mut commands) = tokio::sync::mpsc::channel(8);
    state.controls.sender = Some(sender);
    let mut ui = Ui::new(state, view, (320, 220));
    let volume = slider(&ui);
    assert_eq!(
        ui.harness.access_node(volume).unwrap().min_numeric_value(),
        Some(0.0)
    );
    assert_eq!(
        ui.harness.access_node(volume).unwrap().max_numeric_value(),
        Some(100.0)
    );
    assert!(!text(&ui).contains(&"Brightness and sound controls".into()));
    ui.harness.focus_on(Some(volume));
    assert_eq!(
        ui.harness
            .get_widget_with_id(volume)
            .get_prop::<masonry::properties::BorderColor>()
            .color,
        components::FOCUS_RING_ON_GLASS,
        "the slider focus ring must contrast with translucent panels"
    );
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowRight)));
    ui.dispatch(&view);
    assert_eq!(commands.try_recv().unwrap().action, Action::SetVolume(41));
    assert_eq!(ui.harness.focused_widget_id(), Some(volume));
    assert_eq!(
        ui.harness.access_node(volume).unwrap().numeric_value(),
        Some(41.0)
    );
    ui.harness.process_access_event(ActionRequest {
        action: AccessAction::SetValue,
        target_tree: TreeId::ROOT,
        target_node: volume.to_raw().into(),
        data: Some(ActionData::NumericValue(73.0)),
    });
    ui.dispatch(&view);
    assert_eq!(commands.try_recv().unwrap().action, Action::SetVolume(73));
    ui.activate(ui.button("Mute"));
    ui.dispatch(&view);
    assert_eq!(commands.try_recv().unwrap().action, Action::ToggleMute);
    ui.state.controls.snapshot.audio.as_mut().unwrap().muted = true;
    ui.rebuild(&view);
    ui.dispatch(&view);
    ui.button("Unmute");
    assert!(text(&ui).contains(&"Muted".into()));
    ui.state.controls.volume_pending = None;
    ui.state.controls.volume_error = Some("Audio output rejected the volume change.".into());
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(text(&ui).contains(&"Audio output rejected the volume change.".into()));
    assert!(text(&ui).contains(&"40%".into()));
    assert!(ui.state.outputs[0].show_controls);
}

#[test]
fn missing_audio_disables_slider_and_mute_and_preserves_the_backend_reason() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.toggle_quick_panel("eDP-1", Kind::Sound);
    state.controls.snapshot.audio = Err("No audio output is available.".into());
    let (sender, mut commands) = tokio::sync::mpsc::channel(8);
    state.controls.sender = Some(sender);
    let mut ui = Ui::new(state, view, (280, 220));
    let volume = slider(&ui);
    assert!(ui.harness.access_node(volume).unwrap().is_disabled());
    ui.harness.process_access_event(ActionRequest {
        action: AccessAction::Increment,
        target_tree: TreeId::ROOT,
        target_node: volume.to_raw().into(),
        data: None,
    });
    ui.dispatch(&view);
    assert!(
        ui.harness
            .access_node(ui.button("Mute"))
            .unwrap()
            .is_disabled()
    );
    ui.activate(ui.button("Mute"));
    ui.dispatch(&view);
    assert!(commands.try_recv().is_err());
    assert!(text(&ui).contains(&"No audio output is available.".into()));
    assert!(!text(&ui).contains(&"0%".into()));
}
