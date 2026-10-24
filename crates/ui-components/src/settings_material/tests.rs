use super::*;
use std::sync::Arc;
use xilem::{
    ViewCtx,
    core::{ProxyError, RawProxy, SendMessage, View, ViewId},
    masonry::{
        accesskit::Role,
        core::{
            TextEvent,
            keyboard::{Key, NamedKey},
        },
        layout::AsUnit,
        testing::TestHarness,
        widgets::ButtonPress,
    },
    style::Style,
    view::{button, label, sized_box},
};

#[derive(Debug)]
struct NoMessages;
impl RawProxy for NoMessages {
    fn send_message(&self, _: Arc<[ViewId]>, _: SendMessage) -> Result<(), ProxyError> {
        panic!("material decoration must not start background work")
    }
    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

fn view(selected: bool, disabled: bool) -> impl WidgetView<bool, Widget: Sized> {
    sized_box(
        settings_button(
            button(label("Upload photo"), |state: &mut bool| *state = !*state).height(34.0.px()),
            SettingsButton::Control,
            selected,
        )
        .disabled(disabled),
    )
    .padding(8.0.px())
    .background_color(crate::SURFACE)
}

#[test]
fn material_keeps_native_button_identity_geometry_and_pointer_keyboard_actions() {
    let runtime = xilem::tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
    let mut state = false;
    let initial = view(false, false);
    let (root, mut view_state) = initial.build(&mut ctx, &mut state);
    let mut harness = TestHarness::create_with_size(
        crate::hearthspace_opaque_properties(),
        root.new_widget,
        (160, 58),
    );
    let idle = harness.render();
    harness.press_tab_key(false);
    let native_button = harness.focused_widget_id().unwrap();
    let node = harness.access_node(native_button).unwrap();
    assert_eq!(node.role(), Role::Button);
    let bounds = node.bounding_box().unwrap();
    let focused = harness.render();
    assert_ne!(idle, focused);
    assert!(
        focused.enumerate_pixels().any(|(x, y, pixel)| {
            let outside = f64::from(x) < bounds.x0
                || f64::from(x) > bounds.x1
                || f64::from(y) < bounds.y0
                || f64::from(y) > bounds.y1;
            outside && pixel.0[..3] == [0xe9, 0x64, 0x2a]
        }),
        "keyboard focus must draw outside the native button's measured bounds"
    );
    harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
    harness.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    assert_eq!(
        harness.pop_action::<ButtonPress>().unwrap().1,
        native_button
    );
    harness.mouse_click_on(native_button, None);
    assert_eq!(
        harness.pop_action::<ButtonPress>().unwrap().1,
        native_button
    );
    assert!(
        !harness.render().enumerate_pixels().any(|(x, y, pixel)| {
            let outside = f64::from(x) < bounds.x0
                || f64::from(x) > bounds.x1
                || f64::from(y) < bounds.y0
                || f64::from(y) > bounds.y1;
            outside && pixel.0[..3] == [0xe9, 0x64, 0x2a]
        }),
        "pointer activation keeps native focus without a keyboard outline"
    );

    let selected = view(true, false);
    harness.edit_root_widget(|root| {
        selected.rebuild(&initial, &mut view_state, &mut ctx, root, &mut state);
    });
    harness.redraw();
    assert_eq!(harness.focused_widget_id(), Some(native_button));
    assert_eq!(
        harness.access_node(native_button).unwrap().bounding_box(),
        Some(bounds)
    );

    let disabled = view(true, true);
    harness.edit_root_widget(|root| {
        disabled.rebuild(&selected, &mut view_state, &mut ctx, root, &mut state);
    });
    harness.redraw();
    assert!(harness.access_node(native_button).unwrap().is_disabled());
    harness.mouse_click_on(native_button, None);
    assert!(harness.pop_action::<ButtonPress>().is_none());
}

#[test]
fn reference_material_does_not_change_shell_button_properties() {
    let properties = crate::hearthspace_light_properties();
    let mut harness = TestHarness::create_with_size(
        properties,
        xilem::masonry::core::NewWidget::new(xilem::masonry::widgets::Button::with_text(
            "Shell control",
        )),
        (160, 44),
    );
    harness.press_tab_key(false);
    assert_eq!(
        harness
            .root_widget()
            .get_prop::<xilem::masonry::properties::BorderColor>()
            .color,
        crate::FOCUS_RING_ON_GLASS,
    );
}

#[test]
fn live_palette_repaints_material_and_keeps_native_focus_and_identity() {
    use crate::{Palette, PropertyStyle, hearthspace_properties};
    use hearthspace_common::{AccentColor, Appearance, ColorMode};
    let runtime = xilem::tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
    let mut state = false;
    let (root, _) = view(true, false).build(&mut ctx, &mut state);
    let mut harness = TestHarness::create_with_size(
        hearthspace_properties(Palette::default(), PropertyStyle::Opaque),
        root.new_widget,
        (160, 58),
    );
    harness.press_tab_key(false);
    let button = harness.focused_widget_id().unwrap();
    let bounds = harness.access_node(button).unwrap().bounding_box();
    let before = harness.render();
    let next = Palette::new(Appearance {
        color_mode: ColorMode::Dark,
        accent_color: AccentColor::Teal,
    });
    harness.set_default_properties(Arc::new(hearthspace_properties(
        next,
        PropertyStyle::Opaque,
    )));
    assert_ne!(before, harness.render());
    assert_eq!(harness.focused_widget_id(), Some(button));
    assert_eq!(harness.access_node(button).unwrap().bounding_box(), bounds);
    harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
    harness.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    assert_eq!(harness.pop_action::<ButtonPress>().unwrap().1, button);
}
