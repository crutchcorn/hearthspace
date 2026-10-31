use super::*;
use std::cell::Cell;

#[test]
fn syntax_cache_reuses_only_the_exact_current_optional_key() {
    let mut entry = None;
    let parses = Cell::new(0);
    let parse = |raw: Option<&str>| {
        parses.set(parses.get() + 1);
        Syntax::parse(raw)
    };
    let raw = frame_group_app_id(Some("DP-1"), "system");
    let expected = Syntax::Frame {
        kind: FrameKind::Group,
        output: Some("DP-1".into()),
        target: "system".into(),
    };
    for _ in 0..3 {
        assert_eq!(
            current_syntax(&mut entry, Some(raw.clone()), parse),
            &expected
        );
    }
    assert_eq!(parses.get(), 1);

    // Equivalent decoded bytes are still different raw keys. Never normalize
    // before comparing, or retain an older entry after the current key changes.
    let uppercase = raw.replace("44502d31", "44502D31");
    assert_ne!(uppercase, raw);
    assert_eq!(
        current_syntax(&mut entry, Some(uppercase), parse),
        &expected
    );
    assert_eq!(parses.get(), 2);
    assert_eq!(current_syntax(&mut entry, Some(raw), parse), &expected);
    assert_eq!(parses.get(), 3);

    let changed = frame_group_grip_app_id(None, "tools");
    assert_eq!(
        current_syntax(&mut entry, Some(changed), parse),
        &Syntax::Frame {
            kind: FrameKind::Grip,
            output: None,
            target: "tools".into(),
        }
    );
    assert_eq!(parses.get(), 4);
}

#[test]
fn syntax_cache_reuses_negative_results_without_conflating_missing_and_empty_ids() {
    let mut entry = None;
    let parses = Cell::new(0);
    let parse = |raw: Option<&str>| {
        parses.set(parses.get() + 1);
        Syntax::parse(raw)
    };
    let invalid = "dev.hearthspace.frame-widget.target-zz";
    for (count, raw) in [None, Some(""), Some(invalid), Some("ordinary.app")]
        .into_iter()
        .enumerate()
    {
        for _ in 0..3 {
            assert_eq!(
                current_syntax(&mut entry, raw.map(str::to_owned), parse),
                &Syntax::Other
            );
        }
        assert_eq!(parses.get(), count + 1);
    }
    assert_eq!(
        current_syntax(
            &mut entry,
            Some(hearthspace_common::desktop_item_app_id("app:clock.desktop")),
            parse,
        ),
        &Syntax::DesktopItem("app:clock.desktop".into())
    );
    assert_eq!(parses.get(), 5);
    assert_eq!(
        current_syntax(&mut entry, Some(invalid.into()), parse),
        &Syntax::Other
    );
    assert_eq!(parses.get(), 6);
}
