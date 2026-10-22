use super::widget::TouchScroll;
use masonry::{
    core::{Widget, WindowEvent},
    imaging::{
        record::Scene,
        validation::{ValidatingSink, ValidationError},
    },
    kurbo::Point,
    layout::AsUnit,
    testing::TestHarness,
    widgets::{Portal, SizedBox},
};

fn validate_frame<W: Widget>(harness: &mut TestHarness<W>) -> Result<(), ValidationError> {
    let (plan, _) = harness.redraw();
    let mut sink = ValidatingSink::new(Scene::new());
    plan.replay_into(&mut sink);
    sink.finish()
}

#[test]
fn fitting_content_reveal_then_resize_keeps_scrollbar_paint_finite() {
    let content = SizedBox::empty().width(298.0.px()).height(153.0.px());
    let portal = Portal::new(content.prepare())
        .constrain_horizontal(true)
        .content_must_fill(false);
    let mut harness = TestHarness::create_with_size(
        crate::hearthspace_light_properties(),
        TouchScroll::new(portal.prepare(), true, false).prepare(),
        (298, 153),
    );
    let (content_id, scrollbar_id) = {
        let root = harness.root_widget();
        let portal = root.children()[0];
        let children = portal.children();
        // Portal exposes content, vertical scrollbar, then horizontal scrollbar.
        (children[0].id(), children[1].id())
    };
    assert!(harness.get_widget_with_id(scrollbar_id).ctx().is_stashed());

    // Tray selection requests a reveal even when its popup content fits exactly.
    // Keep the real accessibility-to-Portal path, including RequestPanToChild.
    harness.scroll_into_view(content_id);
    assert_eq!(validate_frame(&mut harness), Ok(()));

    // A smaller configured popup exposes the same scrollbar after that reveal.
    // Validate actual recorded paint without initializing a rasterizer or GPU.
    harness.process_window_event(WindowEvent::Resize((298, 18).into()));
    assert!(!harness.get_widget_with_id(scrollbar_id).ctx().is_stashed());
    let root = harness.root_widget();
    let portal = root.children()[0].downcast::<Portal<SizedBox>>().unwrap();
    assert_eq!(portal.get_viewport_pos(), Point::ORIGIN);
    assert_eq!(validate_frame(&mut harness), Ok(()));
}
