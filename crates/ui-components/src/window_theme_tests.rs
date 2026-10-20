//! Exercise WindowView creation, which a bare widget harness does not cover.
use crate::{Palette, PropertyStyle, SettingsSurface, hearthspace_properties, settings_surface};
use hearthspace_common::{AccentColor, Appearance, ColorMode};
use std::sync::Arc;
use xilem::{
    ViewCtx, WindowId,
    core::{ProxyError, RawProxy, SendMessage, View, ViewId},
    masonry::{
        core::Widget,
        properties::ContentColor,
        testing::TestHarness,
        widgets::{Label, SizedBox},
    },
    view::{button, label, sized_box},
};

#[derive(Debug)]
struct NoMessages;
impl RawProxy for NoMessages {
    fn send_message(&self, _: Arc<[ViewId]>, _: SendMessage) -> Result<(), ProxyError> {
        panic!("static window starts no tasks")
    }
    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

fn first_frame(palette: Palette) -> TestHarness<SizedBox> {
    let runtime = xilem::tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
    let view = xilem::window(
        WindowId::next(),
        "Themed window",
        settings_surface(
            sized_box(button(label("Ready"), |_: &mut ()| {})),
            SettingsSurface::Raised,
        ),
    )
    .with_base_color(palette.app_background)
    .with_default_properties(Arc::new(hearthspace_properties(
        palette,
        PropertyStyle::Glass,
    )));
    let (window, _) = view.build(&mut ctx, &mut ());
    drop(view);
    // NewWindow carries these exact defaults into RenderRoot::new, including
    // windows first created after the app-wide startup theme is obsolete.
    let properties = Arc::try_unwrap(
        window
            .0
            .default_properties
            .expect("WindowView must forward initial defaults"),
    )
    .unwrap();
    TestHarness::create_with_size(
        properties,
        SizedBox::new(window.0.root_widget).prepare(),
        (180, 60),
    )
}
fn label_color(harness: &TestHarness<SizedBox>) -> xilem::Color {
    let mut pending = vec![harness.root_id()];
    while let Some(id) = pending.pop() {
        let widget = harness.get_widget_with_id(id);
        if let Some(label) = widget.downcast::<Label>() {
            return label.get_prop::<ContentColor>().color;
        }
        pending.extend(widget.children().iter().map(|child| child.id()));
    }
    panic!("window contains its native label")
}

#[test]
fn persisted_dark_window_receives_its_theme_before_first_layout_and_paint() {
    let palette = Palette::new(Appearance {
        color_mode: ColorMode::Dark,
        accent_color: AccentColor::Violet,
    });
    let mut window = first_frame(palette);
    assert_eq!(label_color(&window), palette.text);
    let dark = window.render();
    assert_ne!(dark, first_frame(Palette::default()).render());
}

#[test]
fn newly_created_popup_uses_current_theme_after_existing_window_changes() {
    let mut existing = first_frame(Palette::default());
    let palette = Palette::new(Appearance {
        color_mode: ColorMode::Dark,
        accent_color: AccentColor::Teal,
    });
    existing.set_default_properties(Arc::new(hearthspace_properties(
        palette,
        PropertyStyle::Glass,
    )));
    let mut popup = first_frame(palette);
    assert_eq!(label_color(&popup), palette.text);
    assert_eq!(
        popup.render(),
        existing.render(),
        "a new popup's first paint must match the already-updated window"
    );
}
