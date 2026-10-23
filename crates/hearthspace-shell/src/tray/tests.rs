use super::*;

#[path = "protocol_tests.rs"]
mod protocol;
#[path = "race_tests.rs"]
mod races;
#[path = "test_support.rs"]
pub(crate) mod support;

#[test]
fn steam_properties_keep_stable_id_separate_from_title_and_icon() {
    use super::properties::from_properties;
    use zbus::zvariant::{OwnedObjectPath, Value};
    let properties = support::Props::from([
        ("Id".into(), support::text("steam")),
        ("Title".into(), support::text("Localized Steam title")),
        ("IconName".into(), support::text("steam_tray_mono")),
        (
            "Menu".into(),
            Value::from(
                OwnedObjectPath::try_from("/org/ayatana/NotificationItem/steam/Menu").unwrap(),
            )
            .try_into()
            .unwrap(),
        ),
    ]);
    let item = from_properties(
        ItemKey {
            owner: ":1.7".into(),
            path: "/Item".into(),
            generation: 2,
        },
        &properties,
    )
    .unwrap()
    .view;
    assert_eq!(item.id, "steam");
    assert_eq!(item.title, "Localized Steam title");
    assert_eq!(item.icon.name.as_deref(), Some("steam_tray_mono"));
    assert!(!item.item_is_menu);
    assert_eq!(
        item.menu_path.as_deref(),
        Some("/org/ayatana/NotificationItem/steam/Menu")
    );
    assert!(item.menu.is_some());
}

#[test]
fn malformed_pixmaps_are_rejected_and_argb_is_converted() {
    use super::properties::select_pixmap;
    assert!(
        select_pixmap(vec![
            (i32::MAX, 4, vec![]),
            (0, 0, vec![]),
            (3, 3, vec![0; 35])
        ])
        .is_none()
    );
    let image = select_pixmap(vec![(1, 1, vec![128, 11, 22, 33])]).unwrap();
    assert_eq!((image.width, image.height), (1, 1));
    assert_eq!(image.rgba, [11, 22, 33, 128]);
    let image = select_pixmap(vec![
        (24, 24, vec![0; 24 * 24 * 4]),
        (64, 64, vec![0; 64 * 64 * 4]),
        (48, 48, vec![0; 48 * 48 * 4]),
    ])
    .unwrap();
    assert_eq!(image.width, 48);
}

#[test]
fn command_queue_has_a_fixed_capacity() {
    let (handle, _updates, _worker) = channel(None);
    let key = ItemKey {
        owner: ":1.1".into(),
        path: "/Item".into(),
        generation: 1,
    };
    for _ in 0..32 {
        handle
            .try_send(TrayCommand::Activate {
                key: key.clone(),
                x: 0,
                y: 0,
            })
            .unwrap();
    }
    assert!(
        handle
            .try_send(TrayCommand::Activate { key, x: 0, y: 0 })
            .unwrap_err()
            .contains("busy")
    );
}

fn parse_menu(nodes: Vec<support::Layout>) -> Result<TrayMenu, String> {
    let (id, properties, children) = support::node(0, "", nodes);
    super::menu::parse(super::menu::Layout {
        id,
        properties,
        children,
    })
}

#[test]
fn menu_limits_reject_duplicate_ids_large_trees_and_excessive_depth() {
    assert!(
        parse_menu(vec![
            support::node(1, "One", vec![]),
            support::node(1, "Duplicate", vec![])
        ])
        .is_err()
    );
    assert!(
        parse_menu(
            (1..=257)
                .map(|id| support::node(id, "Large", vec![]))
                .collect()
        )
        .is_err()
    );
    let mut deep = support::node(12, "Deep", vec![]);
    for id in (1..12).rev() {
        deep = support::node(id, "Deep", vec![deep]);
    }
    assert!(parse_menu(vec![deep]).is_err());
}

#[test]
fn menu_semantics_preserve_radio_separator_hidden_and_disabled_ancestry() {
    use zbus::zvariant::OwnedValue;
    let mut parent = support::node(
        1,
        "Disabled parent",
        vec![support::node(2, "Child", vec![])],
    );
    parent.1.insert("enabled".into(), OwnedValue::from(false));
    let mut hidden = support::node(3, "Hidden", vec![]);
    hidden.1.insert("visible".into(), OwnedValue::from(false));
    let mut separator = support::node(4, "", vec![]);
    separator
        .1
        .insert("type".into(), support::text("separator"));
    let mut radio = support::node(5, "Radio", vec![]);
    radio.1.insert("toggle-type".into(), support::text("radio"));
    radio
        .1
        .insert("toggle-state".into(), OwnedValue::from(1_i32));
    let parsed = parse_menu(vec![parent, hidden, separator, radio]).unwrap();
    for id in [1, 2, 3, 4] {
        assert!(!super::menu::actionable(&parsed, id, false));
    }
    assert!(!super::menu::actionable(&parsed, 1, true));
    assert!(super::menu::actionable(&parsed, 0, true));
    assert!(super::menu::actionable(&parsed, 5, false));
    assert_eq!(parsed.items[3].toggle, Some(TrayToggle::Radio(true)));
    assert!(parsed.items[2].separator);
}
