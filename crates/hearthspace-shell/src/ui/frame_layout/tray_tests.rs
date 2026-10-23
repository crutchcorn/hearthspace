use super::super::tray as frame_tray;
use super::*;
use masonry::{
    core::{
        PointerButtonEvent, PointerEvent, PointerInfo, PointerState, PointerType, PointerUpdate,
    },
    kurbo::Vec2,
    layout::AsUnit,
};
use xilem::style::Style;

#[path = "tray_centering_tests.rs"]
mod centering;

fn tray_state(vertical: bool) -> ShellState {
    let mut state = state();
    state.tray.snapshot.items = (1..=12).map(crate::ui::tray_tests::item).collect();
    let output = &mut state.frame_layout.snapshot.outputs[0];
    output.rect = Rect {
        width: 260.0,
        height: 220.0,
        ..Default::default()
    };
    output.layout.set_placement(
        &LayoutTarget::widget("tray"),
        ObjectPlacement::frame(
            if vertical {
                FrameEdge::Left
            } else {
                FrameEdge::Top
            },
            0.5,
        ),
    );
    state
}

fn vertical(state: &ShellState) -> bool {
    state.frame_layout.snapshot.outputs[0]
        .layout
        .placement(&LayoutTarget::widget("tray"))
        .edge()
        .is_some_and(|edge| edge.is_vertical())
}

fn tray_size(state: &ShellState) -> (f64, f64) {
    frame_tray::size(
        &state.frame_layout.snapshot.outputs[0],
        vertical(state),
        widgets::size(
            state,
            "tray",
            vertical(state),
            false,
            false,
            Default::default(),
            false,
        ),
    )
}

fn view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    let chain = &state.frame_layout.chains[&widget_key("test", "tray")];
    let size = tray_size(state);
    sized_box(focus::boundary(
        sized_box(widgets::content(
            state,
            &state.outputs[0],
            "tray",
            vertical(state),
            false,
            Default::default(),
            chain,
        ))
        .padding(5.0.px()),
        chain.clone(),
        "test".into(),
        LayoutTarget::widget("tray"),
        state.frame_layout.snapshot.focus_request.clone(),
    ))
    .width(size.0.px())
    .height(size.1.px())
}

fn bounds<V: WidgetView<ShellState>>(ui: &Ui<V>, name: &str) -> masonry::accesskit::Rect
where
    V::Widget: Sized,
{
    ui.harness
        .access_node(ui.button(name))
        .unwrap()
        .bounding_box()
        .unwrap()
}

fn assert_visible<V: WidgetView<ShellState>>(ui: &Ui<V>, name: &str)
where
    V::Widget: Sized,
{
    let bounds = bounds(ui, name);
    let size = tray_size(&ui.state);
    assert!(
        bounds.x0 >= 5.0
            && bounds.y0 >= 5.0
            && bounds.x1 <= size.0 - 5.0
            && bounds.y1 <= size.1 - 5.0,
        "{name} must be inside the tray viewport: {bounds:?}, {size:?}"
    );
}

#[test]
fn standalone_tray_tab_and_reverse_entry_reveal_every_app_in_both_orientations() {
    for vertical in [false, true] {
        let state = tray_state(vertical);
        let size = tray_size(&state);
        let mut ui = Ui::new(state, view, (size.0 as u32, size.1 as u32));
        ui.harness.focus_on(Some(ui.button("App 1")));
        ui.dispatch(&view);
        for index in 1..=12 {
            let name = format!("App {index}");
            assert_eq!(ui.harness.focused_widget_id(), Some(ui.button(&name)));
            assert_visible(&ui, &name);
            if index < 12 {
                ui.harness.press_tab_key(false);
                ui.dispatch(&view);
            }
        }
        for index in (1..12).rev() {
            ui.harness.press_tab_key(true);
            ui.dispatch(&view);
            assert_visible(&ui, &format!("App {index}"));
        }
        ui.state.frame_layout.snapshot.focus_request = Some(FocusRequest {
            serial: 1,
            output: "test".into(),
            target: LayoutTarget::widget("tray"),
            reverse: true,
        });
        ui.rebuild(&view);
        ui.dispatch(&view);
        assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("App 12")));
        assert_visible(&ui, "App 12");
    }
}

#[test]
fn standalone_tray_wheel_reaches_both_ends_without_opening_an_app_menu() {
    for vertical in [false, true] {
        let state = tray_state(vertical);
        let size = tray_size(&state);
        let mut ui = Ui::new(state, view, (size.0 as u32, size.1 as u32));
        ui.harness.mouse_move_to(ui.button("App 1"));
        ui.dispatch(&view);
        ui.harness.mouse_wheel(Vec2::new(0.0, -10_000.0));
        ui.dispatch(&view);
        assert_visible(&ui, "App 12");
        ui.harness.mouse_move_to(ui.button("App 12"));
        ui.harness.mouse_wheel(Vec2::new(0.0, 10_000.0));
        ui.dispatch(&view);
        assert_visible(&ui, "App 1");
        if !vertical {
            ui.harness.mouse_move_to(ui.button("App 1"));
            ui.harness.mouse_wheel(Vec2::new(-10_000.0, 0.0));
            ui.dispatch(&view);
            assert_visible(&ui, "App 12");
        }
        assert!(!ui.state.outputs[0].tray.open);
        assert!(ui.state.outputs[0].tray.error.is_none());
    }
}

fn touch(kind: &str, x: f64, y: f64) -> PointerEvent {
    let pointer = PointerInfo {
        pointer_id: Some(masonry::core::pointer::PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type: PointerType::Touch,
    };
    let state = PointerState {
        position: (x, y).into(),
        ..Default::default()
    };
    match kind {
        "down" => PointerEvent::Down(PointerButtonEvent {
            pointer,
            state,
            button: None,
        }),
        "up" => PointerEvent::Up(PointerButtonEvent {
            pointer,
            state,
            button: None,
        }),
        "move" => PointerEvent::Move(PointerUpdate {
            pointer,
            current: state,
            coalesced: vec![],
            predicted: vec![],
        }),
        _ => unreachable!(),
    }
}

#[test]
fn standalone_tray_touch_drag_scrolls_without_activating_the_captured_app() {
    for vertical in [false, true] {
        let state = tray_state(vertical);
        let size = tray_size(&state);
        let mut ui = Ui::new(state, view, (size.0 as u32, size.1 as u32));
        let before = bounds(&ui, "App 1");
        let (x, y) = ((before.x0 + before.x1) / 2.0, (before.y0 + before.y1) / 2.0);
        let (end_x, end_y) = if vertical {
            (x, y - 12.0)
        } else {
            (x - 12.0, y)
        };
        ui.harness.process_pointer_event(touch("down", x, y));
        ui.harness
            .process_pointer_event(touch("move", end_x, end_y));
        ui.harness.process_pointer_event(touch("up", end_x, end_y));
        ui.dispatch(&view);
        let after = bounds(&ui, "App 1");
        assert!(if vertical {
            after.y0 < before.y0
        } else {
            after.x0 < before.x0
        });
        assert!(!ui.state.outputs[0].tray.open);
        assert!(
            ui.state.outputs[0].tray.error.is_none(),
            "drag dispatched an app action"
        );
    }
}

#[test]
fn standalone_tray_sizes_follow_output_and_live_icons_without_a_stale_surface_cap() {
    let mut state = tray_state(false);
    assert_eq!(tray_size(&state), (236.0, 54.0));
    state.tray.snapshot.items.truncate(2);
    assert_eq!(tray_size(&state), (82.0, 54.0));
    state.tray.snapshot.items = (1..=12).map(crate::ui::tray_tests::item).collect();
    state.frame_layout.snapshot.outputs[0].rect.width = 1280.0;
    assert_eq!(tray_size(&state), (462.0, 54.0));
    state.frame_layout.snapshot.outputs[0].rect.width = 260.0;
    assert_eq!(tray_size(&state), (236.0, 54.0));
}

#[test]
fn fitting_and_grouped_trays_keep_application_wheel_actions() {
    for grouped in [false, true] {
        let mut state = tray_state(false);
        if grouped {
            state.frame_layout.snapshot.outputs[0].layout = Default::default();
        } else {
            state.tray.snapshot.items.truncate(2);
        }
        let size = tray_size(&state);
        let mut ui = Ui::new(state, view, (size.0 as u32, size.1 as u32));
        let before = bounds(&ui, "App 1");
        ui.harness.mouse_move_to(ui.button("App 1"));
        ui.harness.mouse_wheel(Vec2::new(0.0, -120.0));
        ui.dispatch(&view);
        assert_eq!(bounds(&ui, "App 1"), before);
        // No bus worker is installed in this harness. Its unavailable error
        // proves the event retained the application-command route.
        assert!(ui.state.outputs[0].tray.error.is_some());
    }
}
