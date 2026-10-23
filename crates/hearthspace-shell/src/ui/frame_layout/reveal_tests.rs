use super::*;
use crate::ui::{
    frame_layout::{PinnedWindow, Rect, Snapshot, apply},
    test_support::{Ui, ids},
};
use hearthspace_ui_components::icons::IconWidget;
use masonry::{core::WindowEvent, kurbo, widgets::Label};

fn root(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(view(state, &state.frame_layout.snapshot.pinned_windows[0]))
}

#[test]
fn pinned_caption_keeps_glyph_and_title_visible_on_side_and_bottom_edges() {
    for (edge, size) in [
        (FrameEdge::Right, (44, 382)),
        (FrameEdge::Bottom, (382, 44)),
    ] {
        let mut state = ShellState::new("/unused".into(), vec!["test".into()]);
        apply(
            &mut state,
            Snapshot {
                version: 1,
                revision: 1,
                pinned_windows: vec![PinnedWindow {
                    id: 42,
                    title: "Layout review notes".into(),
                    app_id: "test.Notes".into(),
                    output: "test".into(),
                    rect: Rect {
                        x: 0.,
                        y: 0.,
                        width: f64::from(size.0),
                        height: f64::from(size.1),
                    },
                    expanded: false,
                    hovered: false,
                    edge: Some(edge),
                }],
                ..Default::default()
            },
        );
        let (sender, mut requests) = tokio::sync::mpsc::channel(8);
        state.frame_layout.requests = Some(sender);
        let mut ui = Ui::new(state, root, size);
        for scale in [1., 2.] {
            ui.harness.process_window_event(WindowEvent::Rescale(scale));
            ui.harness.process_window_event(WindowEvent::Resize(
                (
                    (f64::from(size.0) * scale) as u32,
                    (f64::from(size.1) * scale) as u32,
                )
                    .into(),
            ));
            ui.dispatch(&root);
            let visible = kurbo::Rect::new(0., 0., f64::from(size.0), f64::from(size.1));
            let mut title = None;
            let mut glyph = None;
            for id in ids(&ui.harness) {
                let widget = ui.harness.get_widget_with_id(id);
                let bounds = widget
                    .ctx()
                    .window_transform()
                    .transform_rect_bbox(widget.ctx().border_box());
                if widget
                    .downcast::<Label>()
                    .is_some_and(|label| label.text().as_ref() == "Layout review notes")
                {
                    title = Some(bounds);
                }
                if widget.downcast::<IconWidget>().is_some() {
                    glyph = Some(bounds);
                }
            }
            let title = title.expect("visible window title");
            let glyph = glyph.expect("visible app glyph");
            for bounds in [title, glyph] {
                assert!(bounds.width() > 0. && bounds.height() > 0.);
                assert!(
                    bounds.x0 >= visible.x0
                        && bounds.y0 >= visible.y0
                        && bounds.x1 <= visible.x1
                        && bounds.y1 <= visible.y1,
                    "caption is outside the visible {edge:?} strip: {bounds:?}"
                );
            }
            assert!((glyph.width() - 18.).abs() < 0.001);
            assert!((glyph.height() - 18.).abs() < 0.001);
            if edge == FrameEdge::Right {
                assert!(title.height() > 80. && title.width() < 20.);
                assert!((title.y0 - glyph.y1 - 10.).abs() < 0.001);
            } else {
                assert!(title.width() > 80. && title.height() < 20.);
                assert!((title.x0 - glyph.x1 - 10.).abs() < 0.001);
            }
        }
        let button = ui.button("Open pinned Layout review notes");
        ui.harness.press_tab_key(false);
        assert_eq!(ui.harness.focused_widget_id(), Some(button));
        ui.activate(button);
        ui.dispatch(&root);
        assert!(
            std::iter::from_fn(|| requests.try_recv().ok())
                .any(|request| request.command == "frame-reveal 42")
        );
    }
}
