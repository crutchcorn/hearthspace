//! Direct device dropdowns keep natural sizing and overflow behavior at every scale.
use super::*;
use crate::{
    controls::{audio::AudioStatus, brightness::BrightnessStatus},
    ui::test_support::{Ui, ids},
};
use masonry::{accesskit::Role, core::WindowEvent};

fn view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(panel(state, &state.outputs[0]))
}

#[test]
fn direct_device_panels_fit_their_measured_window_and_scroll_real_overflow() {
    for (kind, percentage) in [(Kind::Sound, "38%"), (Kind::Brightness, "60%")] {
        for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
            let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
            state.toggle_quick_panel("eDP-1", kind);
            state.controls.snapshot.audio = Ok(AudioStatus {
                volume: 38,
                muted: false,
                output: "Ryzen HD Audio Controller Analog Stereo".into(),
            });
            state.controls.snapshot.brightness = Ok(Some(BrightnessStatus {
                percentage: 60,
                device: "panel".into(),
            }));
            let (sender, _commands) = tokio::sync::mpsc::channel(8);
            state.controls.sender = Some(sender);
            let mut ui = Ui::new(state, view, (320, 200));
            ui.harness.process_window_event(WindowEvent::Rescale(scale));
            ui.harness.process_window_event(WindowEvent::Resize(
                ((320.0 * scale) as u32, (200.0 * scale) as u32).into(),
            ));
            ui.dispatch(&view);
            let normal = resize_to_measured(&mut ui, scale, &view);
            assert!(
                normal < 240.0,
                "{kind:?} should fit a compact dropdown: {normal}"
            );
            assert_natural_content_visible(&ui, percentage);

            if kind == Kind::Sound {
                ui.state.controls.snapshot.audio.as_mut().unwrap().output =
                    "An unusually long audio output description. ".repeat(80);
            } else {
                ui.state.controls.brightness_error =
                    Some("A detailed backlight error. ".repeat(80));
            }
            ui.rebuild(&view);
            ui.dispatch(&view);
            assert_eq!(resize_to_measured(&mut ui, scale, &view), 400.0);
            assert!(
                ids(&ui.harness).into_iter().any(|id| {
                    ui.harness.access_node(id).is_some_and(|node| {
                        node.role() == Role::ScrollView
                            && node.scroll_y_max().is_some_and(|max| max > 0.0)
                    })
                }),
                "genuinely overflowing content must remain scrollable"
            );

            ui.state.controls.snapshot.audio.as_mut().unwrap().output =
                "Ryzen HD Audio Controller Analog Stereo".into();
            ui.state.controls.brightness_error = None;
            ui.rebuild(&view);
            ui.dispatch(&view);
            assert_eq!(resize_to_measured(&mut ui, scale, &view), normal);
            assert_natural_content_visible(&ui, percentage);
        }
    }
}

fn resize_to_measured<V>(ui: &mut Ui<V>, scale: f64, build: &impl Fn(&ShellState) -> V) -> f64
where
    V: WidgetView<ShellState, Widget = masonry::widgets::SizedBox>,
{
    let size = popup::size(&ui.state.outputs[0], PopupKind::Controls, 320.0, 200.0);
    ui.harness.process_window_event(WindowEvent::Resize(
        (
            (size.width * scale).ceil() as u32,
            (size.height * scale).ceil() as u32,
        )
            .into(),
    ));
    ui.dispatch(build);
    assert_eq!(
        popup::size(&ui.state.outputs[0], PopupKind::Controls, 320.0, 200.0),
        size,
        "measured window size must remain stable after applying it"
    );
    size.height
}

fn assert_natural_content_visible<V>(ui: &Ui<V>, expected: &str)
where
    V: WidgetView<ShellState, Widget = masonry::widgets::SizedBox>,
{
    let ids = ids(&ui.harness);
    let viewport = ids
        .iter()
        .find_map(|&id| {
            let node = ui.harness.access_node(id)?;
            if node.role() != Role::ScrollView {
                return None;
            }
            assert!(
                node.scroll_y_max().is_none_or(|max| max <= 0.0),
                "measured popup must not overflow by a fractional pixel"
            );
            node.bounding_box()
        })
        .unwrap();
    for &id in &ids {
        let widget = ui.harness.get_widget_with_id(id);
        if widget.downcast::<masonry::widgets::ScrollBar>().is_some() {
            assert!(
                widget.ctx().is_stashed(),
                "natural content must hide both scrollbars"
            );
        }
    }
    let bounds = ids
        .iter()
        .find_map(|&id| {
            let node = ui.harness.access_node(id)?;
            (node.value().as_deref() == Some(expected))
                .then(|| node.bounding_box())
                .flatten()
        })
        .unwrap_or_else(|| panic!("Missing percentage {expected}"));
    assert!(
        bounds.x0 >= viewport.x0
            && bounds.x1 <= viewport.x1
            && bounds.y0 >= viewport.y0
            && bounds.y1 <= viewport.y1,
        "percentage {expected} must be fully visible: {bounds:?} vs {viewport:?}"
    );
}
