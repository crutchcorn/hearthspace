use super::*;
use crate::{
    controls::{audio::AudioStatus, brightness::BrightnessStatus},
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
    sized_box(panel(&state.outputs[0], &state.controls))
}

fn slider(
    ui: &Ui<impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox>>,
    name: &str,
) -> WidgetId {
    ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.role() == Role::Slider && node.label().as_deref() == Some(name)
            })
        })
        .unwrap_or_else(|| panic!("Missing slider {name}"))
}

#[test]
fn named_sliders_accept_keyboard_and_accessibility_changes_and_mute_is_separate() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.toggle_controls("eDP-1");
    state.controls.snapshot.audio = Ok(AudioStatus {
        volume: 40,
        muted: false,
        output: "Speakers".into(),
    });
    state.controls.snapshot.brightness = Ok(Some(BrightnessStatus {
        percentage: 60,
        device: "panel".into(),
    }));
    let (sender, mut commands) = tokio::sync::mpsc::channel(8);
    state.controls.sender = Some(sender);
    let mut ui = Ui::new(state, view, (340, 260));
    let volume = slider(&ui, "Volume");
    let brightness = slider(&ui, "Brightness");
    assert_eq!(
        ui.harness
            .access_node(brightness)
            .unwrap()
            .min_numeric_value(),
        Some(1.0)
    );
    assert_eq!(
        ui.harness.access_node(volume).unwrap().max_numeric_value(),
        Some(100.0)
    );
    ui.harness.focus_on(Some(volume));
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
        target_node: brightness.to_raw().into(),
        data: Some(ActionData::NumericValue(73.0)),
    });
    ui.dispatch(&view);
    assert_eq!(
        commands.try_recv().unwrap().action,
        Action::SetBrightness(73)
    );
    ui.activate(ui.button("Mute"));
    ui.dispatch(&view);
    assert_eq!(commands.try_recv().unwrap().action, Action::ToggleMute);
    ui.state.controls.snapshot.audio.as_mut().unwrap().muted = true;
    ui.rebuild(&view);
    ui.harness.redraw();
    ui.button("Unmute");
    assert!(ui.state.outputs[0].show_controls);
}

#[test]
fn unavailable_devices_disable_slider_and_mute_without_fabricating_zero_readings() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.controls.snapshot.audio = Err("No audio output is available.".into());
    state.controls.snapshot.brightness = Ok(None);
    let (sender, mut commands) = tokio::sync::mpsc::channel(8);
    state.controls.sender = Some(sender);
    let mut ui = Ui::new(state, view, (340, 280));
    for name in ["Brightness", "Volume"] {
        let id = slider(&ui, name);
        assert!(ui.harness.access_node(id).unwrap().is_disabled());
        ui.harness.process_access_event(ActionRequest {
            action: AccessAction::Increment,
            target_tree: TreeId::ROOT,
            target_node: id.to_raw().into(),
            data: None,
        });
        ui.dispatch(&view);
    }
    assert!(
        ui.harness
            .access_node(ui.button("Mute"))
            .unwrap()
            .is_disabled()
    );
    assert!(commands.try_recv().is_err());
    let text = ids(&ui.harness)
        .into_iter()
        .filter_map(|id| ui.harness.access_node(id))
        .flat_map(|node| [node.label(), node.value()])
        .flatten()
        .collect::<Vec<_>>();
    assert!(
        text.iter()
            .any(|text| text == "This display has no adjustable backlight.")
    );
    assert!(!text.iter().any(|text| text == "0%"));
}
