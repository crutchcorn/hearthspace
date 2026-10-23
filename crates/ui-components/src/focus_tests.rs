use xilem::masonry::{
    core::{NewWidget, Widget},
    layout::AsUnit,
    properties::{BorderColor, Padding},
    testing::TestHarness,
    widgets::{Checkbox, RadioButton, RadioGroup, SizedBox, Slider, TextInput},
};

fn check_native_control(name: &str, widget: impl Widget) {
    let child = NewWidget::new(widget);
    let id = child.id();
    // Native checkbox/radio/slider widgets paint focus outside their bounds.
    // Include surrounding app spacing instead of clipping at the window edge.
    // Shell's NamedSlider separately tests its inside-bounds ring at zero inset.
    let root = NewWidget::new(SizedBox::new(child)).with_props(Padding::all(4.0.px()));
    let mut harness =
        TestHarness::create_with_size(crate::hearthspace_light_properties(), root, (180, 44));
    let idle = harness.render();
    harness.press_tab_key(false);
    assert!(harness.focused_widget_id().is_some(), "{name}: Tab focus");
    assert_eq!(
        harness
            .get_widget_with_id(id)
            .get_prop::<BorderColor>()
            .color,
        crate::FOCUS_RING_ON_GLASS
    );
    assert!(
        idle != harness.render(),
        "{name}: Tab must remain visibly focused"
    );
}

#[test]
fn native_choices_editors_and_sliders_share_the_orange_keyboard_focus_style() {
    check_native_control("Checkbox", Checkbox::new(false, "Allow notifications"));
    let mut radio = TestHarness::create_with_size(
        crate::hearthspace_light_properties(),
        NewWidget::new(RadioGroup::new(NewWidget::new(RadioButton::new(
            false, "Fit",
        )))),
        (180, 44),
    );
    let idle = radio.render();
    radio.press_tab_key(false);
    let focused = radio
        .focused_widget_id()
        .expect("Tab focuses the radio choice");
    assert_eq!(
        radio
            .get_widget_with_id(focused)
            .get_prop::<BorderColor>()
            .color,
        crate::FOCUS_RING_ON_GLASS,
    );
    assert!(
        idle != radio.render(),
        "RadioButton: Tab must remain visibly focused"
    );
    check_native_control("TextInput", TextInput::new("Editable text"));
    check_native_control("Slider", Slider::new(0.0, 100.0, 50.0));
}

#[test]
fn opaque_app_buttons_use_the_reference_orange_for_visible_focus() {
    let mut harness = TestHarness::create_with_size(
        crate::hearthspace_opaque_properties(),
        NewWidget::new(xilem::masonry::widgets::Button::with_text("Photo fit")),
        (180, 44),
    );
    let idle = harness.render();
    harness.press_tab_key(false);
    assert_eq!(
        harness.root_widget().get_prop::<BorderColor>().color,
        crate::FOCUS_RING
    );
    assert!(
        idle != harness.render(),
        "Button: Tab must remain visibly focused"
    );
}
