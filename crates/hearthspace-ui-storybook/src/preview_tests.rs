//! Regression for the actual gallery container, not just isolated widgets.
use crate::{shared, state::AppState};
use std::sync::Arc;
use xilem::{
    ViewCtx,
    core::{ProxyError, RawProxy, SendMessage, View, ViewId},
    masonry::{accesskit::Role, testing::TestHarness},
    view::sized_box,
};

#[derive(Debug)]
struct NoMessages;
impl RawProxy for NoMessages {
    fn send_message(&self, _: Arc<[ViewId]>, _: SendMessage) -> Result<(), ProxyError> {
        panic!("control previews must not start background work")
    }
    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

#[test]
fn gallery_keeps_reference_control_bounds_inside_the_fixed_preview_panel() {
    for (story, role, name, width, height) in [
        ("button--neutral", Role::Button, "Cancel", None, 36.),
        ("button--primary", Role::Button, "Connect", None, 36.),
        (
            "button--icon-only",
            Role::Button,
            "Close dialog",
            Some(36.),
            36.,
        ),
        (
            "toggle-switch--on",
            Role::Switch,
            "Bluetooth",
            Some(40.),
            24.,
        ),
        (
            "checkbox--checked",
            Role::CheckBox,
            "Metered connection",
            Some(20.),
            20.,
        ),
        (
            "select--automatic",
            Role::ComboBox,
            "DNS configuration",
            Some(220.),
            42.,
        ),
        (
            "level-bar--meter",
            Role::Meter,
            "Battery charge",
            Some(256.),
            8.,
        ),
        (
            "level-slider--adjustable",
            Role::Slider,
            "Sound",
            Some(236.),
            32.,
        ),
    ] {
        let mut state = AppState::new();
        let index = state
            .stories()
            .iter()
            .position(|s| s.id() == format!("shared-controls-{story}"))
            .unwrap();
        state.select_story(index);
        let runtime = xilem::tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
        let view = sized_box(shared::preview(&state));
        let (root, _) = view.build(&mut ctx, &mut state);
        let mut h = TestHarness::create_with_size(
            crate::components::hearthspace_opaque_properties(),
            root.new_widget,
            (600, 320),
        );
        let mut ids = Vec::new();
        h.inspect_widgets(|w| ids.push(w.id()));
        let node = ids
            .into_iter()
            .find_map(|id| {
                h.access_node(id)
                    .filter(|n| n.role() == role && n.label().as_deref() == Some(name))
            })
            .unwrap_or_else(|| panic!("missing {story}"));
        let bounds = node.bounding_box().unwrap();
        assert!(
            (bounds.height() - height).abs() < 0.01,
            "{story}: {bounds:?}"
        );
        if let Some(width) = width {
            assert!((bounds.width() - width).abs() < 0.01, "{story}: {bounds:?}");
        }
    }
}

#[test]
fn navigating_all_stories_remeasures_retained_widgets_and_applies_live_themes() {
    let runtime = xilem::tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
    let mut state = AppState::new();
    let mut view = sized_box(crate::app_logic(&mut state));
    let (root, mut view_state) = view.build(&mut ctx, &mut state);
    let mut h = TestHarness::create_with_size(
        crate::components::hearthspace_opaque_properties(),
        root.new_widget,
        (1100, 760),
    );
    for index in 0..state.stories().len() {
        state.select_story(index);
        state.dark = index % 2 == 0;
        state.accent = index % 6;
        state.update_theme();
        h.set_default_properties(state.properties.clone());
        let next = sized_box(crate::app_logic(&mut state));
        h.edit_root_widget(|root| next.rebuild(&view, &mut view_state, &mut ctx, root, &mut state));
        view = next;
        h.render();
    }
}

#[test]
fn adding_a_button_icon_keeps_content_width_when_the_preview_has_spare_room() {
    for with_icon in [false, true] {
        let mut state = AppState::new();
        let index = state
            .stories()
            .iter()
            .position(|s| s.id() == "shared-controls-button--primary")
            .unwrap();
        state.select_story(index);
        state.set_selected_bool("with-icon", with_icon);
        let runtime = xilem::tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
        let (root, _) = sized_box(shared::preview(&state)).build(&mut ctx, &mut state);
        let mut h = TestHarness::create_with_size(
            crate::components::hearthspace_opaque_properties(),
            root.new_widget,
            (800, 320),
        );
        let mut ids = Vec::new();
        h.inspect_widgets(|w| ids.push(w.id()));
        let bounds = ids
            .into_iter()
            .find_map(|id| {
                h.access_node(id)
                    .filter(|n| n.role() == Role::Button && n.label().as_deref() == Some("Connect"))
                    .and_then(|n| n.bounding_box())
            })
            .unwrap();
        assert!(
            bounds.width() > 50. && bounds.width() < 200.,
            "icon={with_icon}: {bounds:?}"
        );
        assert_eq!(bounds.height(), 36.);
    }
}
