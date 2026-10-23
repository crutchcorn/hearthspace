//! Battery arrivals and compositor resize acknowledgements must not make the
//! compact presentation determine its own permanent width.
use super::*;
use crate::{
    battery::{BatteryKind, BatteryState, BatteryStatus},
    ui::test_support::{Ui, ids},
};

fn system(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(system_view(state, &state.outputs[0]))
}

fn state() -> ShellState {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.clock = "Sat Oct 17 02:53".into();
    state
}

fn battery(percentage: u8) -> BatteryStatus {
    BatteryStatus {
        percentage,
        state: BatteryState::Discharging,
        kind: BatteryKind::Battery,
        on_battery: true,
        warning: None,
    }
}

fn settle<V: WidgetView<ShellState>>(
    ui: &mut Ui<V>,
    build: &impl Fn(&ShellState) -> V,
    available: u32,
) where
    V::Widget: Sized,
{
    // Model the compositor acknowledging each requested size, with an output
    // cap. Bound the loop to catch alternate full/compact resize oscillation.
    for _ in 0..8 {
        let requested = system_width(&ui.state, &ui.state.outputs[0]);
        ui.harness
            .process_window_event(masonry::core::WindowEvent::Resize(
                ((requested as u32).min(available), 54).into(),
            ));
        ui.dispatch(build);
        if system_width(&ui.state, &ui.state.outputs[0]) == requested {
            return;
        }
    }
    panic!("system corner did not settle after resize acknowledgements");
}

fn has_percentage<V: WidgetView<ShellState>>(ui: &Ui<V>, percentage: u8) -> bool
where
    V::Widget: Sized,
{
    let text = format!("{percentage}%");
    ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.label()
                .or_else(|| node.value())
                .as_deref()
                .is_some_and(|value| value.contains(&text))
        })
    })
}

#[test]
fn battery_arriving_after_clock_sizing_exposes_percentages_across_digit_counts() {
    let mut ui = Ui::new(state(), system, (320, 54));
    settle(&mut ui, &system, 600);
    assert!(ui.state.outputs[0].corner_widths[1] == 290.0);
    ui.state.battery = Some(battery(57));
    ui.rebuild(&system);
    ui.dispatch(&system);
    settle(&mut ui, &system, 600);
    assert!(
        has_percentage(&ui, 57),
        "a late battery must not latch a compact indicator"
    );
    ui.state.battery = Some(battery(3));
    ui.rebuild(&system);
    ui.dispatch(&system);
    settle(&mut ui, &system, 600);
    assert!(has_percentage(&ui, 3));
    ui.state.battery = Some(battery(100));
    ui.rebuild(&system);
    ui.dispatch(&system);
    settle(&mut ui, &system, 600);
    assert!(has_percentage(&ui, 100));
}

#[test]
fn battery_available_before_first_frame_retains_accessible_percentage() {
    let mut state = state();
    state.battery = Some(battery(57));
    let mut ui = Ui::new(state, system, (320, 54));
    settle(&mut ui, &system, 600);
    assert!(has_percentage(&ui, 57));
}

#[test]
fn constrained_battery_keeps_full_preferred_width_and_expands_when_output_grows() {
    let mut state = state();
    state.battery = Some(battery(57));
    let mut ui = Ui::new(state, system, (180, 54));
    settle(&mut ui, &system, 180);
    let preferred = system_width(&ui.state, &ui.state.outputs[0]);
    assert!(preferred > 180.0);
    assert!(!has_percentage(&ui, 57));
    ui.button("Brightness settings");
    ui.button("Sound settings");
    ui.button("Power");
    settle(&mut ui, &system, 600);
    assert!(has_percentage(&ui, 57));
    assert_eq!(system_width(&ui.state, &ui.state.outputs[0]), preferred);
    ui.state.battery = None;
    ui.rebuild(&system);
    ui.dispatch(&system);
    settle(&mut ui, &system, 600);
    assert!(system_width(&ui.state, &ui.state.outputs[0]) < preferred);
}

#[test]
fn tiny_system_corner_keeps_sound_power_and_application_tray_inside_the_surface() {
    let mut state = state();
    state.battery = Some(battery(57));
    state.tray.snapshot.items = (1..=20).map(crate::ui::tray_tests::item).collect();
    let mut ui = Ui::new(state, system, (132, 54));
    settle(&mut ui, &system, 132);
    assert!(!ids(&ui.harness).into_iter().any(|id| {
        ui.harness
            .access_node(id)
            .is_some_and(|node| node.label().as_deref() == Some("Control center"))
    }));
    for name in ["Application tray", "Sound settings", "Power"] {
        let button = ui.button(name);
        let bounds = ui
            .harness
            .access_node(button)
            .unwrap()
            .bounding_box()
            .unwrap();
        assert!(bounds.x0 >= 0.0 && bounds.x1 <= 132.0, "{name}: {bounds:?}");
        ui.harness.mouse_click_on(button, None);
        ui.dispatch(&system);
        let output = &ui.state.outputs[0];
        assert_eq!(
            [output.tray.open, output.show_controls, output.show_power]
                .into_iter()
                .filter(|open| *open)
                .count(),
            1
        );
        if name == "Sound settings" {
            assert_eq!(output.quick_panel, crate::ui::quick_panel::Kind::Sound);
        }
    }
}
