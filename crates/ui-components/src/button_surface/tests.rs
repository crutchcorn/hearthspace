use super::*;
use std::sync::Arc;
use xilem::{
    core::{ProxyError, RawProxy, SendMessage, ViewId},
    masonry::{
        core::{
            TextEvent,
            keyboard::{Key, NamedKey},
        },
        testing::TestHarness,
        widgets::ButtonPress,
    },
    view::{button, label},
};

#[derive(Debug)]
struct NoMessages;
impl RawProxy for NoMessages {
    fn send_message(&self, _: Arc<[ViewId]>, _: SendMessage) -> Result<(), ProxyError> {
        panic!("button styling must not start background work")
    }
    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

fn view(selected: bool, surface: ButtonSurface) -> impl WidgetView<bool, Widget = Button> {
    button_surface(
        button(label("Toggle"), |state: &mut bool| *state = !*state),
        surface,
        selected,
    )
}

#[test]
fn selected_surface_rebuild_keeps_hover_focus_geometry_and_activation() {
    let runtime = xilem::tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
    let mut state = false;
    let before = view(state, ButtonSurface::Control);
    let (root, mut view_state) = before.build(&mut ctx, &mut state);
    let mut harness = TestHarness::create_with_size(
        crate::hearthspace_light_properties(),
        root.new_widget,
        (100, 44),
    );
    let id = harness.root_id();
    let bounds = harness.access_node(id).unwrap().bounding_box();
    assert_eq!(
        *harness.root_widget().get_prop::<Background>(),
        Background::Color(crate::SURFACE_CONTROL)
    );
    harness.mouse_move_to(id);
    assert_eq!(
        *harness.root_widget().get_prop::<Background>(),
        Background::Color(crate::BUTTON_HOVER)
    );
    harness.focus_on(Some(id));
    assert_eq!(
        harness.root_widget().get_prop::<BorderColor>().color,
        crate::TEXT
    );

    state = true;
    let selected = view(state, ButtonSurface::Control);
    harness.edit_root_widget(|root| {
        selected.rebuild(&before, &mut view_state, &mut ctx, root, &mut state)
    });
    harness.redraw();
    assert_eq!(harness.focused_widget_id(), Some(id));
    assert_eq!(harness.access_node(id).unwrap().bounding_box(), bounds);
    assert_eq!(
        *harness.root_widget().get_prop::<Background>(),
        Background::Color(crate::ACCENT_SOFT)
    );
    assert_eq!(
        harness.root_widget().get_prop::<BorderColor>().color,
        crate::TEXT,
        "selection must not mask keyboard focus"
    );
    harness.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    assert!(harness.pop_action::<ButtonPress>().is_some());
}

#[test]
fn raised_controls_have_accent_hover_without_an_opaque_idle_tile() {
    let runtime = xilem::tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
    let (root, _) = view(false, ButtonSurface::Raised).build(&mut ctx, &mut false);
    let mut harness = TestHarness::create_with_size(
        crate::hearthspace_light_properties(),
        root.new_widget,
        (44, 44),
    );
    assert_eq!(
        *harness.root_widget().get_prop::<Background>(),
        Background::Color(xilem::Color::TRANSPARENT)
    );
    harness.mouse_move_to(harness.root_id());
    assert_eq!(
        *harness.root_widget().get_prop::<Background>(),
        Background::Color(crate::RAISED_BUTTON_HOVER)
    );
}

#[test]
fn primary_action_keeps_visible_focus_and_disables_without_hover_emphasis() {
    let runtime = xilem::tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
    let (root, _) = view(false, ButtonSurface::Primary).build(&mut ctx, &mut false);
    let mut harness = TestHarness::create_with_size(
        crate::hearthspace_light_properties(),
        root.new_widget,
        (120, 44),
    );
    let id = harness.root_id();
    let idle = harness.render();
    harness.focus_on(Some(id));
    assert_ne!(idle, harness.render());
    harness.mouse_move_to(id);
    assert_eq!(
        *harness.root_widget().get_prop::<Background>(),
        Background::Color(crate::ACCENT_STRONG.with_alpha(0.9))
    );
    harness.edit_root_widget(|mut root| root.ctx.set_disabled(true));
    harness.redraw();
    assert_eq!(
        *harness.root_widget().get_prop::<Background>(),
        Background::Color(crate::SURFACE_CONTROL)
    );
    harness.mouse_click_on(id, None);
    assert!(harness.pop_action::<ButtonPress>().is_none());
}
