use super::*;
use crate::{
    controls::brightness::BrightnessStatus,
    ui::test_support::{Ui, ids},
};
use hearthspace_ui_components as components;
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
                node.role() == Role::Slider && node.label().as_deref() == Some("Brightness")
            })
        })
        .expect("named brightness slider")
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
fn brightness_dropdown_routes_keyboard_and_accessibility_to_the_existing_backlight_control() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.toggle_quick_panel("eDP-1", Kind::Brightness);
    state.controls.snapshot.brightness = Ok(Some(BrightnessStatus {
        percentage: 60,
        device: "panel".into(),
    }));
    let (sender, mut commands) = tokio::sync::mpsc::channel(8);
    state.controls.sender = Some(sender);
    let mut ui = Ui::new(state, view, (320, 240));
    let brightness = slider(&ui);
    assert_eq!(
        ui.harness
            .access_node(brightness)
            .unwrap()
            .min_numeric_value(),
        Some(1.0)
    );
    assert_eq!(
        ui.harness
            .access_node(brightness)
            .unwrap()
            .max_numeric_value(),
        Some(100.0)
    );
    assert!(text(&ui).contains(&"60%".into()));
    assert!(
        !text(&ui)
            .iter()
            .any(|text| text.contains("Adjust the display")
                || text.contains("Brightness and sound controls"))
    );
    ui.harness.focus_on(Some(brightness));
    assert_eq!(
        ui.harness
            .get_widget_with_id(brightness)
            .get_prop::<masonry::properties::BorderColor>()
            .color,
        components::FOCUS_RING_ON_GLASS,
    );
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowRight)));
    ui.dispatch(&view);
    let request = commands.try_recv().unwrap();
    assert_eq!(request.action, Action::SetBrightness(61));
    assert_eq!(request.output, "eDP-1");
    assert_eq!(request.token, None);
    assert_eq!(ui.harness.focused_widget_id(), Some(brightness));
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
    assert!(text(&ui).contains(&"73%".into()));
    ui.state.controls.brightness_pending = None;
    ui.state.controls.brightness_error =
        Some("The display rejected this brightness change.".into());
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(text(&ui).contains(&"The display rejected this brightness change.".into()));
    assert!(text(&ui).contains(&"60%".into()));
    assert_eq!(ui.state.outputs[0].quick_panel, Kind::Brightness);
    assert!(ui.state.outputs[0].show_controls);
}

#[test]
fn missing_backlight_and_native_capability_errors_are_visible_and_cannot_queue_changes() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.toggle_quick_panel("eDP-1", Kind::Brightness);
    state.controls.snapshot.brightness = Ok(None);
    let (sender, mut commands) = tokio::sync::mpsc::channel(8);
    state.controls.sender = Some(sender);
    let mut ui = Ui::new(state, view, (280, 240));
    let brightness = slider(&ui);
    assert!(ui.harness.access_node(brightness).unwrap().is_disabled());
    assert!(text(&ui).contains(&"This display has no adjustable backlight.".into()));
    assert!(!text(&ui).contains(&"0%".into()));
    ui.harness.process_access_event(ActionRequest {
        action: AccessAction::Increment,
        target_tree: TreeId::ROOT,
        target_node: brightness.to_raw().into(),
        data: None,
    });
    ui.dispatch(&view);
    assert!(commands.try_recv().is_err());
    let reason = "Brightness is available in a native Hearthspace session.";
    ui.state.controls.snapshot.brightness = Err(reason.into());
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(text(&ui).contains(&reason.into()));
    assert!(ui.harness.access_node(slider(&ui)).unwrap().is_disabled());
    ui.state.controls.snapshot.brightness = Ok(Some(BrightnessStatus {
        percentage: 37,
        device: "panel".into(),
    }));
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(!ui.harness.access_node(slider(&ui)).unwrap().is_disabled());
    assert!(text(&ui).contains(&"37%".into()));
    assert!(!text(&ui).contains(&reason.into()));
}
