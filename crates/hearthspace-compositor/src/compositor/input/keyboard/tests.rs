use super::*;

#[test]
fn shell_corner_shortcuts_support_reverse_and_consume_releases_after_focus_changes() {
    let mut shortcuts = KeyboardShortcuts::default();
    let mut mods = ModifiersState {
        logo: true,
        ..Default::default()
    };
    let key = 72_u32.into();
    assert!(matches!(
        shortcuts.filter(key, KeyState::Pressed, &mods, keysyms::KEY_F6, true),
        FilterResult::Intercept(Some(Shortcut::ShellCornerCycle { reverse: false }))
    ));
    assert!(matches!(
        shortcuts.filter(
            key,
            KeyState::Released,
            &Default::default(),
            keysyms::KEY_F6,
            true
        ),
        FilterResult::Intercept(None)
    ));
    mods.shift = true;
    assert_eq!(
        shortcut(&mods, keysyms::KEY_F6),
        Some(Shortcut::ShellCornerCycle { reverse: true })
    );
    mods.ctrl = true;
    assert_eq!(shortcut(&mods, keysyms::KEY_F6), None);
    assert_eq!(shortcut(&Default::default(), keysyms::KEY_F6), None);
}

#[test]
fn cancelled_search_keeps_preexisting_releases_and_suppresses_new_pairs() {
    let mut shortcuts = KeyboardShortcuts::default();
    let shift = 50_u32.into();
    assert!(matches!(
        shortcuts.discard_pending_key(shift, KeyState::Released, true),
        FilterResult::Forward
    ));
    let a = 38_u32.into();
    assert!(matches!(
        shortcuts.discard_pending_key(a, KeyState::Pressed, false),
        FilterResult::Intercept(())
    ));
    assert!(matches!(
        shortcuts.filter(
            a,
            KeyState::Released,
            &Default::default(),
            keysyms::KEY_a,
            true
        ),
        FilterResult::Intercept(None)
    ));
    // A shortcut press was already intercepted before opening the search.
    shortcuts.intercepted.insert(shift);
    assert!(matches!(
        shortcuts.discard_pending_key(shift, KeyState::Released, true),
        FilterResult::Intercept(())
    ));
}

#[test]
fn canvas_map_escape_closes_once_and_consumes_release_after_focus_changes() {
    let mut shortcuts = KeyboardShortcuts::default();
    let mods = ModifiersState::default();
    let key = 9_u32.into();
    assert!(matches!(
        shortcuts.filter_shell_panel(key, KeyState::Pressed, &mods, keysyms::KEY_Escape, true),
        FilterResult::Intercept(Some(Shortcut::ShellPanelDismiss))
    ));
    assert!(matches!(
        shortcuts.filter_shell_panel(key, KeyState::Pressed, &mods, keysyms::KEY_Escape, true),
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
        shortcuts.filter_shell_panel(key, KeyState::Pressed, &mods, keysyms::KEY_Escape, false),
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
    assert_eq!(shortcut(&mods, keysyms::KEY_0), None);
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
