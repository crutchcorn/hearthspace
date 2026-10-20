use super::*;
use masonry::{
    core::Widget,
    kurbo::Shape,
    peniko::Fill,
    testing::TestHarness,
    widgets::{Button, ButtonPress},
};

#[test]
fn bundled_svg_paths_and_variants_fit_the_shared_drawing_grid() {
    for kind in kind::ALL {
        for filled in [false, true] {
            let geometry = Geometry::new(*kind, filled);
            assert!(!geometry.0.is_empty(), "{kind:?} must draw a glyph");
            for part in geometry.0 {
                let bounds = part.path.bounding_box();
                assert!(
                    bounds.x0 >= 0.0 && bounds.y0 >= 0.0 && bounds.x1 <= 24.0 && bounds.y1 <= 24.0,
                    "{kind:?} ({filled}): {bounds:?}"
                );
            }
        }
    }
}

#[test]
fn filled_document_keeps_real_cutouts_and_controls_keep_their_outline() {
    let outline = Geometry::new(Icon::FileText, false);
    let filled = Geometry::new(Icon::FileText, true);
    assert!(outline.0.iter().all(|part| part.fill.is_none()));
    assert!(filled.0.iter().any(|part| part.fill == Some(Fill::EvenOdd)));
    assert_ne!(outline.0[0].path, filled.0[0].path);
    let settings_control = Geometry::new(Icon::Settings, true);
    assert!(settings_control.0.iter().all(|part| part.fill.is_none()));
}

fn widget(view: IconView) -> IconWidget {
    IconWidget {
        geometry: Geometry::new(view.kind, view.filled),
        view,
    }
}

#[test]
fn named_icons_name_native_buttons_without_changing_activation() {
    let glyph = widget(icon(Icon::Settings, 20.0).label("Open Settings"));
    let mut harness = TestHarness::create_with_size(
        crate::hearthspace_light_properties(),
        Button::new(glyph.prepare()).prepare(),
        (44, 44),
    );
    let root = harness.root_id();
    assert_eq!(
        harness.access_node(root).unwrap().label().as_deref(),
        Some("Open Settings")
    );
    harness.mouse_click_on(root, None);
    assert!(harness.pop_action::<ButtonPress>().is_some());
}

#[test]
fn decorative_icons_are_hidden_from_assistive_technology() {
    let harness = TestHarness::create_with_size(
        crate::hearthspace_light_properties(),
        widget(icon(Icon::Appearance, 20.0)).prepare(),
        (20, 20),
    );
    assert!(harness.access_node(harness.root_id()).unwrap().is_hidden());
}
