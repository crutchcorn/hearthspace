use super::{
    navigation_focus_tests::has_header_ring,
    navigation_tests::{bounds, key, next_button},
    tests::{state, view},
};
use crate::{
    state::{SettingsApp, SettingsCategory},
    test_support::{Ui, ids},
};
use hearthspace_ipc::{AccentColor, ColorMode, shortcuts::CANVAS_SHORTCUTS};
use masonry::{
    accesskit::{Action, AriaCurrent, Role},
    core::{WidgetId, keyboard::NamedKey},
};
use xilem::WidgetView;

const POINTER_SHORTCUTS: &[(&str, &str, &str)] = &[
    (
        "Temporarily pan canvas",
        "❖ + Left-drag",
        "Hold ❖ before left-dragging empty canvas to pan without changing the selected tool. Panning ends when you release the mouse button, even if you release ❖ first.",
    ),
    (
        "Zoom canvas",
        "❖ + Scroll",
        "Hold ❖ and scroll to zoom the canvas.",
    ),
];

fn shortcut_text() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
    CANVAS_SHORTCUTS
        .iter()
        .zip(["❖ + V", "❖ + H", "❖ + K", "❖ + G", "❖ + A", "❖ + O"])
        .map(|(shortcut, chord)| (shortcut.label, chord, shortcut.description))
        .chain(POINTER_SHORTCUTS.iter().copied())
}

fn text<V: WidgetView<SettingsApp>>(ui: &Ui<V>, value: &str) -> WidgetId
where
    V::Widget: Sized,
{
    ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.role() == Role::Label
                    && node.label().or_else(|| node.value()).as_deref() == Some(value)
            })
        })
        .unwrap_or_else(|| panic!("Missing accessible text: {value}"))
}

fn tab_to_page_scroll<V: WidgetView<SettingsApp>>(ui: &mut Ui<V>)
where
    V::Widget: Sized,
{
    for _ in 0..20 {
        ui.harness.press_tab_key(false);
        let Some(id) = ui.harness.focused_widget_id() else {
            continue;
        };
        let node = ui.harness.access_node(id).unwrap();
        assert_ne!(node.role(), Role::Label, "read-only rows are not Tab stops");
        if node.role() == Role::ScrollView && bounds(ui, id).width() > 200.0 {
            return;
        }
    }
    panic!("Tab must reach the page's native scrolling control");
}

#[test]
fn keyboard_navigation_opens_read_only_shortcuts_and_scrolls_to_pointer_shortcuts() {
    for mode in [ColorMode::Light, ColorMode::Dark] {
        for width in [400, 420, 560, 760] {
            let (_dir, mut state) = state();
            state.settings.appearance.color_mode = mode;
            let mut ui = Ui::new(state, view, (width, 420));
            if width < 560 {
                ui.harness.focus_on(Some(ui.button("Settings categories")));
                key(&mut ui, NamedKey::Enter);
                ui.dispatch(&view);
            }
            ui.harness.focus_on(Some(ui.button("Gestures")));
            assert_eq!(next_button(&mut ui, false), ui.button("Keyboard shortcuts"));
            key(&mut ui, NamedKey::Enter);
            ui.dispatch(&view);
            assert_eq!(
                ui.state.selected_category,
                SettingsCategory::KeyboardShortcuts
            );
            if width < 560 {
                let opener = ui.button("Settings categories");
                assert!(!ui.state.navigation.open);
                assert_eq!(ui.harness.focused_widget_id(), Some(opener));
                assert!(has_header_ring(&mut ui, opener));
            } else {
                let category = ui.button("Keyboard shortcuts");
                assert_eq!(ui.harness.focused_widget_id(), Some(category));
                assert_eq!(
                    ui.harness.access_node(category).unwrap().aria_current(),
                    Some(AriaCurrent::Page)
                );
            }
            for (name, chord, description) in shortcut_text() {
                for value in [name, chord, description] {
                    let node = ui.harness.access_node(text(&ui, value)).unwrap();
                    assert!(!node.data().supports_action(Action::Click));
                    assert!(!node.data().supports_action(Action::Focus));
                    assert!(!node.is_disabled(), "shortcut text remains readable");
                }
            }
            text(
                &ui,
                "❖ means Meta (Super, Windows, or Command). Hold it and press a key. You can release both keys after choosing a tool.",
            );
            text(&ui, "These shortcuts cannot be changed yet.");
            text(&ui, "Keyboard");
            text(&ui, "Pointer shortcuts");
            text(
                &ui,
                "Hold ❖ for these pointer gestures; they do not select a tool.",
            );

            let last = text(&ui, POINTER_SHORTCUTS.last().unwrap().2);
            assert!(bounds(&ui, last).y1 > 420.0, "the page needs scrolling");
            tab_to_page_scroll(&mut ui);
            key(&mut ui, NamedKey::End);
            ui.dispatch(&view);
            for &(name, chord, description) in POINTER_SHORTCUTS {
                for value in [name, chord, description] {
                    let visible = bounds(&ui, text(&ui, value));
                    assert!(
                        visible.y0 >= 49.0 && visible.y1 <= 420.0,
                        "{value} must be reachable by keyboard at width {width}: {visible:?}"
                    );
                }
            }
            key(&mut ui, NamedKey::Home);
            ui.dispatch(&view);
            let first = text(&ui, &CANVAS_SHORTCUTS[0].chord());
            assert!(bounds(&ui, first).y1 < 420.0);
        }
    }
}

#[test]
fn shortcut_text_and_chords_fit_compact_and_sidebar_widths_in_both_color_modes() {
    for mode in [ColorMode::Light, ColorMode::Dark] {
        for width in [400, 420, 559, 560, 760] {
            let (_dir, mut state) = state();
            state.settings.appearance.color_mode = mode;
            state.selected_category = SettingsCategory::KeyboardShortcuts;
            let ui = Ui::new(state, view, (width, 420));
            let page_left = if width < 560 { 0.0 } else { 160.0 };
            for (label, chord, description) in shortcut_text() {
                let name = bounds(&ui, text(&ui, label));
                let chord = bounds(&ui, text(&ui, chord));
                let description = bounds(&ui, text(&ui, description));
                for rect in [name, chord, description] {
                    assert!(
                        rect.x0 >= page_left && rect.x1 <= f64::from(width),
                        "{label} is clipped at width {width}: {rect:?}"
                    );
                    assert!(rect.width() > 0.0 && rect.height() > 0.0);
                }
                assert!(name.x1 < chord.x0, "name and chord must not overlap");
                assert!(
                    description.y0 >= name.y1.max(chord.y1),
                    "description follows its action and chord"
                );
            }
            if width >= 560 {
                let category = bounds(&ui, ui.button("Keyboard shortcuts"));
                for id in ids(&ui.harness) {
                    let Some(node) = ui.harness.access_node(id) else {
                        continue;
                    };
                    if node.role() == Role::Label
                        && node.label().or_else(|| node.value()).as_deref()
                            == Some("Keyboard shortcuts")
                    {
                        let label = bounds(&ui, id);
                        if label.x0 < 160.0 {
                            assert!(
                                category.contains(label.origin())
                                    && label.x1 <= category.x1
                                    && label.y1 <= category.y1,
                                "the full sidebar category label must fit: {label:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn live_appearance_changes_repaint_shortcuts_without_replacing_content_or_focus() {
    let (_dir, mut state) = state();
    state.selected_category = SettingsCategory::KeyboardShortcuts;
    let mut ui = Ui::new(state, view, (400, 420));
    let opener = ui.button("Settings categories");
    ui.harness.focus_on(Some(opener));
    let content: Vec<_> = shortcut_text()
        .map(|(name, chord, _)| (text(&ui, name), text(&ui, chord)))
        .collect();
    tab_to_page_scroll(&mut ui);
    key(&mut ui, NamedKey::End);
    ui.dispatch(&view);
    ui.harness.focus_on(Some(opener));
    let mut previous = ui.harness.render();
    for mode in [ColorMode::Dark, ColorMode::Light] {
        ui.state.settings.appearance.color_mode = mode;
        ui.state.settings.appearance.accent_color = AccentColor::Blue;
        ui.rebuild(&view);
        ui.dispatch(&view);
        assert_eq!(
            ui.state.selected_category,
            SettingsCategory::KeyboardShortcuts
        );
        assert_eq!(ui.harness.focused_widget_id(), Some(opener));
        assert!(has_header_ring(&mut ui, opener));
        for ((label, binding, _), (name, chord)) in shortcut_text().zip(&content) {
            assert_eq!(text(&ui, label), *name);
            assert_eq!(text(&ui, binding), *chord);
        }
        for &(name, _, description) in POINTER_SHORTCUTS {
            assert!(bounds(&ui, text(&ui, name)).y0 >= 49.0);
            assert!(bounds(&ui, text(&ui, description)).y1 <= 420.0);
        }
        let painted = ui.harness.render();
        assert!(previous != painted, "live appearance must repaint the page");
        previous = painted;
    }
}
