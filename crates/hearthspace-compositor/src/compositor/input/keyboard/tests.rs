use super::*;

#[test]
fn canvas_map_escape_closes_once_and_consumes_release_after_focus_changes() {
    let mut shortcuts = KeyboardShortcuts::default();
    let mods = ModifiersState::default();
    let key = 9_u32.into();
    assert!(matches!(
        shortcuts.filter_canvas_map(key, KeyState::Pressed, &mods, keysyms::KEY_Escape, true),
        FilterResult::Intercept(Some(Shortcut::CanvasMapDismiss))
    ));
    assert!(matches!(
        shortcuts.filter_canvas_map(key, KeyState::Pressed, &mods, keysyms::KEY_Escape, true),
        FilterResult::Intercept(None)
    ));
    assert!(matches!(
        shortcuts.filter(key, KeyState::Released, &mods, keysyms::KEY_Escape, true),
        FilterResult::Intercept(None)
    ));
    assert!(matches!(
        shortcuts.filter(key, KeyState::Pressed, &mods, keysyms::KEY_Escape, true),
        FilterResult::Forward
    ));
    assert!(matches!(
        shortcuts.filter_canvas_map(key, KeyState::Pressed, &mods, keysyms::KEY_Escape, false),
        FilterResult::Forward
    ));
}

#[test]
fn shortcuts_respect_modifiers_and_leave_application_keys_alone() {
    let mut mods = ModifiersState::default();
    assert_eq!(shortcut(&mods, keysyms::KEY_Return), None);
    mods.logo = true;
    assert_eq!(
        shortcut(&mods, keysyms::KEY_Return),
        Some(Shortcut::Terminal)
    );
    assert_eq!(shortcut(&mods, keysyms::KEY_0), Some(Shortcut::Reset));
    assert_eq!(shortcut(&mods, keysyms::KEY_l), Some(Shortcut::Lock));
    assert_eq!(shortcut(&mods, keysyms::KEY_a), None);
    mods.ctrl = true;
    assert_eq!(shortcut(&mods, keysyms::KEY_Return), None);
    mods = ModifiersState {
        alt: true,
        ..Default::default()
    };
    assert_eq!(shortcut(&mods, keysyms::KEY_Tab), Some(Shortcut::Next));
    mods.shift = true;
    assert_eq!(shortcut(&mods, keysyms::KEY_Tab), Some(Shortcut::Previous));
}

#[test]
fn release_is_consumed_after_modifier_released_and_repeats_do_not_retrigger() {
    let mut shortcuts = KeyboardShortcuts::default();
    let mods = ModifiersState {
        logo: true,
        ..Default::default()
    };
    let key = 36_u32.into();
    assert!(matches!(
        shortcuts.filter(key, KeyState::Pressed, &mods, keysyms::KEY_Return, true),
        FilterResult::Intercept(Some(Shortcut::Terminal))
    ));
    assert!(matches!(
        shortcuts.filter(key, KeyState::Pressed, &mods, keysyms::KEY_Return, true),
        FilterResult::Intercept(None)
    ));
    assert!(matches!(
        shortcuts.filter(
            key,
            KeyState::Released,
            &ModifiersState::default(),
            keysyms::KEY_Return,
            false
        ),
        FilterResult::Intercept(None)
    ));
    assert!(matches!(
        shortcuts.filter(
            key,
            KeyState::Pressed,
            &ModifiersState::default(),
            keysyms::KEY_Return,
            true
        ),
        FilterResult::Forward
    ));
}

#[test]
fn popup_grabs_keep_shortcuts_and_their_releases() {
    let mut shortcuts = KeyboardShortcuts::default();
    let mods = ModifiersState {
        alt: true,
        ..Default::default()
    };
    assert!(matches!(
        shortcuts.filter(
            23_u32.into(),
            KeyState::Pressed,
            &mods,
            keysyms::KEY_Tab,
            false
        ),
        FilterResult::Forward
    ));
    assert!(matches!(
        shortcuts.filter(
            23_u32.into(),
            KeyState::Released,
            &mods,
            keysyms::KEY_Tab,
            true
        ),
        FilterResult::Forward
    ));
}

#[test]
fn overview_consumes_typing_and_release_after_dismissal() {
    let mut shortcuts = KeyboardShortcuts::default();
    let mods = ModifiersState::default();
    let key = 38_u32.into();
    assert!(matches!(
        shortcuts.filter_overview(key, KeyState::Pressed, &mods, keysyms::KEY_a),
        FilterResult::Intercept(None)
    ));
    assert!(matches!(
        shortcuts.filter(key, KeyState::Released, &mods, keysyms::KEY_a, true),
        FilterResult::Intercept(None)
    ));
    assert_eq!(
        overview_shortcut(&mods, keysyms::KEY_Escape),
        Some(Shortcut::OverviewDismiss)
    );
    assert_eq!(
        overview_shortcut(&mods, keysyms::KEY_Return),
        Some(Shortcut::OverviewActivate)
    );
    assert_eq!(
        overview_shortcut(&mods, keysyms::KEY_Right),
        Some(Shortcut::OverviewNext)
    );
    assert_eq!(
        overview_shortcut(
            &ModifiersState {
                shift: true,
                ..Default::default()
            },
            keysyms::KEY_Tab
        ),
        Some(Shortcut::OverviewPrevious)
    );
    assert_eq!(
        overview_shortcut(
            &ModifiersState {
                logo: true,
                ..Default::default()
            },
            keysyms::KEY_l
        ),
        Some(Shortcut::Lock)
    );
    assert_eq!(
        shortcut(
            &ModifiersState {
                logo: true,
                ..Default::default()
            },
            keysyms::KEY_o
        ),
        Some(Shortcut::OverviewToggle)
    );
}
