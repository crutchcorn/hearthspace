//! Check the production tray inside the surface allocation used by frame parts.
use super::*;
use masonry::core::WindowEvent;

fn grouped_state(vertical: bool, count: usize) -> ShellState {
    let mut state = tray_state(vertical);
    state.tray.snapshot.items.truncate(count);
    let layout = &mut state.frame_layout.snapshot.outputs[0].layout;
    *layout = Default::default();
    layout.set_placement(
        &LayoutTarget::new(TargetKind::Group, "system"),
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

fn grouped_size(state: &ShellState) -> (f64, f64) {
    widgets::size(
        state,
        "tray",
        vertical(state),
        false,
        true,
        Default::default(),
        false,
    )
}

fn grouped_view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    let chain = &state.frame_layout.chains[&widget_key("test", "tray")];
    let size = grouped_size(state);
    // Group members occupy their own 44px lane without standalone outer padding.
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
        .padding(0.0.px()),
        chain.clone(),
        "test".into(),
        LayoutTarget::widget("tray"),
        state.frame_layout.snapshot.focus_request.clone(),
    ))
    .width(size.0.px())
    .height(size.1.px())
}

fn assert_short_axis_centered<V: WidgetView<ShellState>>(
    ui: &Ui<V>,
    name: &str,
    vertical: bool,
    padding: f64,
) where
    V::Widget: Sized,
{
    let rect = bounds(ui, name);
    let surface = ui
        .harness
        .get_widget_with_id(ui.harness.root_id())
        .ctx()
        .border_box();
    let (center, allocated) = if vertical {
        ((rect.x0 + rect.x1) / 2.0, surface.width())
    } else {
        ((rect.y0 + rect.y1) / 2.0, surface.height())
    };
    assert_eq!(allocated, 44.0 + 2.0 * padding, "allocated tray lane");
    assert_eq!(
        (rect.width(), rect.height()),
        (34.0, 34.0),
        "{name} keeps its button size: {rect:?}"
    );
    assert_eq!(
        center,
        padding + 22.0,
        "{name} must be centered across the tray lane: {rect:?}"
    );
}

fn assert_fitting_tray_centered<V: WidgetView<ShellState>>(
    ui: &Ui<V>,
    count: usize,
    vertical: bool,
    padding: f64,
) where
    V::Widget: Sized,
{
    let surface = ui
        .harness
        .get_widget_with_id(ui.harness.root_id())
        .ctx()
        .border_box();
    let width = surface.width();
    let height = surface.height();
    if count == 1 {
        let side = 44.0 + 2.0 * padding;
        assert_eq!((width, height), (side, side), "single-icon surface");
    }
    for index in 1..=count {
        let name = format!("App {index}");
        assert_short_axis_centered(ui, &name, vertical, padding);
        let rect = bounds(ui, &name);
        assert!(
            rect.x0 >= padding
                && rect.y0 >= padding
                && rect.x1 <= width - padding
                && rect.y1 <= height - padding,
            "{name} must fit inside the tray: {rect:?}"
        );
    }
    let first = bounds(ui, "App 1");
    let last = bounds(ui, &format!("App {count}"));
    assert_eq!(
        ((first.x0 + last.x1) / 2.0, (first.y0 + last.y1) / 2.0),
        (width / 2.0, height / 2.0),
        "the button strip must be centered on both axes"
    );
}

fn check_standalone(vertical: bool, count: usize, expected_size: (f64, f64)) {
    let mut state = tray_state(vertical);
    state.tray.snapshot.items.truncate(count);
    let size = tray_size(&state);
    assert_eq!(size, expected_size, "standalone surface allocation");
    let ui = Ui::new(state, view, (size.0 as u32, size.1 as u32));
    assert_fitting_tray_centered(&ui, count, vertical, 5.0);
}

fn check_grouped(vertical: bool, count: usize, expected_size: (f64, f64)) {
    let state = grouped_state(vertical, count);
    let size = grouped_size(&state);
    assert_eq!(size, expected_size, "group member surface allocation");
    let ui = Ui::new(state, grouped_view, (size.0 as u32, size.1 as u32));
    assert_fitting_tray_centered(&ui, count, vertical, 0.0);
}

#[test]
fn horizontal_standalone_single_icon_is_centered_on_both_axes() {
    check_standalone(false, 1, (54.0, 54.0));
}

#[test]
fn vertical_standalone_single_icon_is_centered_on_both_axes() {
    check_standalone(true, 1, (54.0, 54.0));
}

#[test]
fn horizontal_grouped_single_icon_is_centered_on_both_axes() {
    check_grouped(false, 1, (44.0, 44.0));
}

#[test]
fn vertical_grouped_single_icon_is_centered_on_both_axes() {
    check_grouped(true, 1, (44.0, 44.0));
}

#[test]
fn horizontal_standalone_several_icons_are_centered_in_the_lane() {
    check_standalone(false, 3, (120.0, 54.0));
}

#[test]
fn vertical_standalone_several_icons_are_centered_in_the_lane() {
    check_standalone(true, 3, (54.0, 120.0));
}

#[test]
fn horizontal_grouped_several_icons_are_centered_in_the_lane() {
    check_grouped(false, 3, (110.0, 44.0));
}

#[test]
fn vertical_grouped_several_icons_are_centered_in_the_lane() {
    check_grouped(true, 3, (44.0, 110.0));
}

fn check_overflow_centering(vertical: bool) {
    let state = tray_state(vertical);
    let size = tray_size(&state);
    let mut ui = Ui::new(state, view, (size.0 as u32, size.1 as u32));
    assert_visible(&ui, "App 1");
    assert_short_axis_centered(&ui, "App 1", vertical, 5.0);
    let tail = bounds(&ui, "App 12");
    assert!(
        if vertical {
            tail.y1 > size.1 - 5.0
        } else {
            tail.x1 > size.0 - 5.0
        },
        "the last icon must initially overflow: {tail:?}"
    );

    ui.harness.mouse_move_to(ui.button("App 1"));
    ui.harness.mouse_wheel(Vec2::new(0.0, -10_000.0));
    ui.dispatch(&view);
    assert_visible(&ui, "App 12");
    assert_short_axis_centered(&ui, "App 12", vertical, 5.0);

    ui.harness.mouse_move_to(ui.button("App 12"));
    ui.harness.mouse_wheel(Vec2::new(0.0, 10_000.0));
    ui.dispatch(&view);
    assert_visible(&ui, "App 1");
    assert_short_axis_centered(&ui, "App 1", vertical, 5.0);
}

#[test]
fn horizontal_overflow_scrolling_keeps_buttons_centered_across_the_lane() {
    check_overflow_centering(false);
}

#[test]
fn vertical_overflow_scrolling_keeps_buttons_centered_across_the_lane() {
    check_overflow_centering(true);
}

fn rebuild_standalone<V>(ui: &mut Ui<V>, build: &impl Fn(&ShellState) -> V)
where
    V: WidgetView<ShellState, Widget = masonry::widgets::SizedBox>,
{
    ui.rebuild(build);
    let size = tray_size(&ui.state);
    ui.harness
        .process_window_event(WindowEvent::Resize((size.0 as u32, size.1 as u32).into()));
    ui.dispatch(build);
}

#[test]
fn standalone_centering_survives_live_count_and_orientation_rebuilds() {
    let mut state = tray_state(false);
    state.tray.snapshot.items.truncate(1);
    let size = tray_size(&state);
    let mut ui = Ui::new(state, view, (size.0 as u32, size.1 as u32));
    assert_fitting_tray_centered(&ui, 1, false, 5.0);

    ui.state.tray.snapshot.items = (1..=3).map(crate::ui::tray_tests::item).collect();
    rebuild_standalone(&mut ui, &view);
    assert_fitting_tray_centered(&ui, 3, false, 5.0);

    ui.state.frame_layout.snapshot.outputs[0]
        .layout
        .set_placement(
            &LayoutTarget::widget("tray"),
            ObjectPlacement::frame(FrameEdge::Left, 0.5),
        );
    rebuild_standalone(&mut ui, &view);
    assert_fitting_tray_centered(&ui, 3, true, 5.0);

    ui.state.tray.snapshot.items.truncate(1);
    rebuild_standalone(&mut ui, &view);
    assert_fitting_tray_centered(&ui, 1, true, 5.0);
}
