use super::*;
use crate::{controls::audio::AudioStatus, ui::test_support::Ui};
use image::RgbaImage;
use masonry::{
    core::{
        WindowEvent,
        keyboard::{Key, NamedKey},
    },
    kurbo::Affine,
    peniko::Color,
};

fn view(state: &ShellState) -> ControlSlider {
    ControlSlider {
        name: "Volume",
        output: "eDP-1".into(),
        minimum: 0,
        value: state
            .controls
            .snapshot
            .audio
            .as_ref()
            .map_or(50, |audio| audio.volume),
        enabled: state.controls.snapshot.audio.is_ok(),
    }
}

fn fixture() -> (
    Ui<ControlSlider>,
    tokio::sync::mpsc::Receiver<super::super::control_state::Request>,
) {
    fixture_at_scale(1.0)
}

fn fixture_at_scale(
    scale: f64,
) -> (
    Ui<ControlSlider>,
    tokio::sync::mpsc::Receiver<super::super::control_state::Request>,
) {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.controls.snapshot.audio = Ok(AudioStatus {
        volume: 50,
        muted: false,
        output: "Speakers".into(),
    });
    let (sender, commands) = tokio::sync::mpsc::channel(16);
    state.controls.sender = Some(sender);
    let size = ((220.0 * scale) as u32, (32.0 * scale) as u32);
    let mut ui = Ui::new(state, view, size);
    ui.harness.process_window_event(WindowEvent::Rescale(scale));
    // Distinguish the focus decoration from the unchanged upstream thumb/track.
    ui.harness.edit_root_widget(|mut widget| {
        widget.insert_prop(BorderColor {
            color: Color::from_rgb8(255, 0, 255),
        });
        // TestHarness renders logical scenes without the window renderer's
        // scale transform. Apply it here to check the physical pixels too.
        widget.set_transform(Affine::scale(scale));
    });
    (ui, commands)
}

fn ring_pixels(image: &RgbaImage) -> Vec<(u32, u32)> {
    image
        .enumerate_pixels()
        .filter_map(|(x, y, pixel)| {
            let [r, g, b, _] = pixel.0;
            (r > 180 && g < 100 && b > 180).then_some((x, y))
        })
        .collect()
}

fn assert_ring(ui: &mut Ui<ControlSlider>, visible: bool) {
    assert_eq!(!ring_pixels(&ui.harness.render()).is_empty(), visible);
}

#[test]
fn pointer_hover_click_and_drag_keep_semantic_focus_without_an_outline() {
    let (mut ui, mut commands) = fixture();
    let id = ui.harness.root_id();
    ui.harness.mouse_move_to(id);
    assert_ring(&mut ui, false);
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    assert_eq!(ui.harness.focused_widget_id(), Some(id));
    assert_ring(&mut ui, false);
    ui.harness.mouse_move((165.0, 16.0));
    ui.dispatch(&view);
    assert_eq!(commands.try_recv().unwrap().action, Action::SetVolume(75));
    assert_ring(&mut ui, false);
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    assert_ring(&mut ui, false);
    assert_eq!(
        ui.harness.access_node(id).unwrap().numeric_value(),
        Some(75.0)
    );
}

#[test]
fn keyboard_shows_an_outline_after_pointer_focus_and_preserves_it_across_rebuilds() {
    let (mut ui, mut commands) = fixture();
    let id = ui.harness.root_id();
    ui.harness.mouse_click_on(id, Some(PointerButton::Primary));
    assert_ring(&mut ui, false);
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowRight)));
    ui.dispatch(&view);
    assert_eq!(commands.try_recv().unwrap().action, Action::SetVolume(51));
    assert_eq!(
        ui.harness.access_node(id).unwrap().numeric_value(),
        Some(51.0)
    );
    assert_ring(&mut ui, true);

    ui.state.controls.snapshot.audio.as_mut().unwrap().volume = 62;
    ui.rebuild(&view);
    ui.harness.redraw();
    assert_eq!(
        ui.harness.access_node(id).unwrap().numeric_value(),
        Some(62.0)
    );
    assert_ring(&mut ui, true);
    ui.harness.mouse_click_on(id, Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert_ring(&mut ui, false);
    assert_eq!(ui.harness.focused_widget_id(), Some(id));
}

#[test]
fn tab_focus_outline_is_fully_inside_all_four_edges_at_fractional_scales() {
    for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
        let (mut ui, _commands) = fixture_at_scale(scale);
        let width = (220.0 * scale) as u32;
        let height = (32.0 * scale) as u32;
        ui.harness.press_tab_key(false);
        assert_eq!(ui.harness.focused_widget_id(), Some(ui.harness.root_id()));
        let pixels = ring_pixels(&ui.harness.render());
        assert!(!pixels.is_empty());
        let left = pixels.iter().map(|(x, _)| *x).min().unwrap();
        let right = pixels.iter().map(|(x, _)| *x).max().unwrap();
        let top = pixels.iter().map(|(_, y)| *y).min().unwrap();
        let bottom = pixels.iter().map(|(_, y)| *y).max().unwrap();
        let expected_inset = (scale * 2.0).ceil() as u32;
        assert!(
            left >= 1 && left <= expected_inset,
            "left edge at {scale}x: {left}"
        );
        assert!(
            top >= 1 && top <= expected_inset,
            "top edge at {scale}x: {top}"
        );
        assert!(
            right < width - 1 && right >= width - expected_inset - 1,
            "right edge at {scale}x: {right}"
        );
        assert!(
            bottom < height - 1 && bottom >= height - expected_inset - 1,
            "bottom edge at {scale}x: {bottom}"
        );
        // Both vertical segments must reach the center, rather than being
        // clipped away while the horizontal segments remain visible.
        assert!(pixels.iter().any(|(x, y)| *x == left && *y == height / 2));
        assert!(pixels.iter().any(|(x, y)| *x == right && *y == height / 2));
    }
}

#[test]
fn ignored_secondary_click_does_not_suppress_later_keyboard_focus() {
    let (mut ui, _commands) = fixture();
    let id = ui.harness.root_id();
    ui.harness
        .mouse_click_on(id, Some(PointerButton::Secondary));
    assert_eq!(ui.harness.focused_widget_id(), None);
    assert_ring(&mut ui, false);
    ui.harness.press_tab_key(false);
    assert_eq!(ui.harness.focused_widget_id(), Some(id));
    assert_ring(&mut ui, true);
}

#[test]
fn focus_loss_and_disabled_state_remove_the_outline() {
    let (mut ui, mut commands) = fixture();
    ui.harness.press_tab_key(false);
    assert_ring(&mut ui, true);
    ui.harness
        .process_text_event(TextEvent::WindowFocusChange(false));
    assert_ring(&mut ui, false);
    ui.harness
        .process_text_event(TextEvent::WindowFocusChange(true));
    assert_ring(&mut ui, true);
    ui.harness.focus_on(None);
    assert_ring(&mut ui, false);
    ui.harness.press_tab_key(false);
    assert_ring(&mut ui, true);

    ui.state.controls.snapshot.audio = Err("No audio output".into());
    ui.rebuild(&view);
    ui.harness.redraw();
    assert_ring(&mut ui, false);
    assert!(
        ui.harness
            .access_node(ui.harness.root_id())
            .unwrap()
            .is_disabled()
    );
    ui.harness.mouse_move((165.0, 16.0));
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowRight)));
    ui.dispatch(&view);
    assert_ring(&mut ui, false);
    assert!(commands.try_recv().is_err());
}
