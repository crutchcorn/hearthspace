use super::*;
use crate::ui::{corners, test_support::Ui};
use hearthspace_common::{AccentColor, Appearance, ColorMode};
use xilem::{
    WidgetView,
    masonry::{
        core::{
            TextEvent,
            keyboard::{Key, NamedKey},
        },
        widgets::SizedBox,
    },
    view::sized_box,
};

fn apps(state: &ShellState) -> impl WidgetView<ShellState, Widget = SizedBox> + use<> {
    sized_box(corners::apps_view(state, &state.outputs[0]))
}

#[test]
fn unchanged_appearance_reuses_properties_and_changed_palette_repaints_open_controls() {
    let state = ShellState::new("/unused".into(), vec!["test-output".into()]);
    let mut ui = Ui::new(state, apps, (54, 54));
    ui.state.outputs[0].query = "preserve query".into();
    ui.harness.press_tab_key(false);
    let focused = ui.harness.focused_widget_id().unwrap();
    let bounds = ui.harness.access_node(focused).unwrap().bounding_box();
    let before = ui.harness.render();
    let old = ui.state.appearance.properties.clone();
    ui.state.appearance.update(Palette::default());
    assert!(Arc::ptr_eq(&old, &ui.state.appearance.properties));
    let palette = Palette::new(Appearance {
        color_mode: ColorMode::Dark,
        accent_color: AccentColor::Blue,
    });
    ui.state.appearance.update(palette);
    assert!(!Arc::ptr_eq(&old, &ui.state.appearance.properties));
    ui.rebuild(&apps);
    assert_ne!(before, ui.harness.render());
    let mut pending = vec![ui.harness.root_id()];
    let mut found_icon = false;
    while let Some(id) = pending.pop() {
        let widget = ui.harness.get_widget_with_id(id);
        if let Some(icon) = widget.downcast::<hearthspace_ui_components::icons::IconWidget>() {
            assert_eq!(
                icon.get_prop::<xilem::masonry::properties::ContentColor>()
                    .color,
                palette.text_on_glass,
                "retained corner glyphs use shell ink, not the reference text token"
            );
            found_icon = true;
        }
        pending.extend(widget.children().iter().map(|child| child.id()));
    }
    assert!(found_icon, "the Apps List button retains its native icon");
    assert_eq!(ui.harness.focused_widget_id(), Some(focused));
    assert_eq!(
        ui.harness.access_node(focused).unwrap().bounding_box(),
        bounds
    );
    assert_eq!(ui.state.outputs[0].query, "preserve query");
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&apps);
    assert!(
        ui.state.outputs[0].show_search,
        "theme updates must preserve native keyboard activation"
    );
}
