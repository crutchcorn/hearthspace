use super::*;
use masonry::accesskit::Rect;

#[test]
fn parent_only_action_changes_register_remove_and_reindex_child_actions() {
    let mut root = Node::new(Role::Window);
    root.set_children([TARGET]);
    root.set_bounds(Rect::new(0.0, 0.0, 100.0, 100.0));
    let mut fixture = Fixture::with_root(root.clone(), button(&[]));
    let node = fixture.target();
    fixture.assert_actions(&node, &[]);

    // Every update below contains only the parent; geometry and children stay put.
    root.add_child_action(Action::ShowContextMenu);
    fixture.update(WINDOW, root.clone());
    fixture.assert_actions(&node, &["showContextMenu"]);
    assert!(node.do_action(0).unwrap());
    fixture
        .requests
        .assert_received(TreeId::ROOT, TARGET, &[Action::ShowContextMenu]);

    root.add_child_action(Action::Click);
    fixture.update(WINDOW, root.clone());
    fixture.assert_actions(&node, &["click", "showContextMenu"]);
    assert!(node.do_action(0).unwrap());
    assert!(node.do_action(1).unwrap());
    fixture.requests.assert_received(
        TreeId::ROOT,
        TARGET,
        &[Action::Click, Action::ShowContextMenu],
    );

    root.remove_child_action(Action::Click);
    fixture.update(WINDOW, root.clone());
    fixture.assert_actions(&node, &["showContextMenu"]);
    fixture.assert_invalid_indices(&node, 1);
    assert!(node.do_action(0).unwrap());
    fixture
        .requests
        .assert_received(TreeId::ROOT, TARGET, &[Action::ShowContextMenu]);

    root.remove_child_action(Action::ShowContextMenu);
    fixture.update(WINDOW, root.clone());
    fixture.assert_actions(&node, &[]);
    fixture.assert_invalid_indices(&node, 0);

    root.add_child_action(Action::Click);
    fixture.update(WINDOW, root.clone());
    fixture.assert_actions(&node, &["click"]);
    assert!(node.do_action(0).unwrap());
    fixture
        .requests
        .assert_received(TreeId::ROOT, TARGET, &[Action::Click]);

    root.remove_child_action(Action::Click);
    fixture.update(WINDOW, root.clone());
    fixture.assert_actions(&node, &[]);
    fixture.assert_invalid_indices(&node, 0);

    // Unmapped child actions must not introduce an AT-SPI Action interface.
    root.add_child_action(Action::Focus);
    fixture.update(WINDOW, root);
    fixture.assert_actions(&node, &[]);
    fixture.assert_invalid_indices(&node, 0);
}

#[test]
fn inherited_actions_follow_transparent_children_and_preserve_clipping() {
    const VISIBLE: LocalId = LocalId(8);
    const CLIP: LocalId = LocalId(9);
    const CLIPPED: LocalId = LocalId(10);
    let mut fixture = Fixture::new(button(&[]));
    let mut transparent = Node::new(Role::GenericContainer);
    transparent.set_children([VISIBLE, CLIP]);
    let mut clip = Node::new(Role::ScrollView);
    clip.set_bounds(Rect::new(0.0, 0.0, 100.0, 50.0));
    clip.set_clips_children();
    clip.set_children([CLIPPED]);
    let mut clipped = button(&[]);
    clipped.set_bounds(Rect::new(0.0, 100.0, 20.0, 120.0));
    fixture.adapter.update(TreeUpdate {
        tree_id: TreeId::ROOT,
        tree: None,
        nodes: vec![
            (TARGET, transparent),
            (VISIBLE, button(&[])),
            (CLIP, clip.clone()),
            (CLIPPED, clipped.clone()),
        ],
        focus: WINDOW,
    });
    let root = fixture.adapter.platform_node(fixture.adapter.root_id());
    let visible = fixture.target();
    let viewport = fixture
        .adapter
        .platform_node(root.child_at_index(1).unwrap().unwrap());
    assert_eq!(root.child_count().unwrap(), 2);
    assert_eq!(viewport.child_count().unwrap(), 0);
    fixture.assert_actions(&visible, &[]);

    let mut parent = Node::new(Role::Window);
    parent.set_children([TARGET]);
    parent.add_child_action(Action::ShowContextMenu);
    fixture.update(WINDOW, parent.clone());
    fixture.assert_actions(&visible, &["showContextMenu"]);
    fixture.assert_actions(&viewport, &["showContextMenu"]);
    assert!(visible.do_action(0).unwrap());
    fixture
        .requests
        .assert_received(TreeId::ROOT, VISIBLE, &[Action::ShowContextMenu]);

    parent.clear_child_actions();
    fixture.update(WINDOW, parent);
    fixture.assert_actions(&visible, &[]);
    fixture.assert_actions(&viewport, &[]);
    fixture.assert_invalid_indices(&visible, 0);

    clip.add_child_action(Action::ShowContextMenu);
    fixture.update(CLIP, clip);
    assert_eq!(viewport.child_count().unwrap(), 0);
    assert_eq!(
        fixture
            .registrations
            .0
            .lock()
            .unwrap()
            .values()
            .filter(|interfaces| interfaces.bits() != 0)
            .count(),
        3,
        "only the window, visible button, and viewport may be registered"
    );

    // Existing clipping refresh must expose the inherited action on entry.
    clipped.set_bounds(Rect::new(0.0, 0.0, 20.0, 20.0));
    fixture.update(CLIPPED, clipped);
    let revealed = fixture
        .adapter
        .platform_node(viewport.child_at_index(0).unwrap().unwrap());
    fixture.assert_actions(&revealed, &["showContextMenu"]);
    assert!(revealed.do_action(0).unwrap());
    fixture
        .requests
        .assert_received(TreeId::ROOT, CLIPPED, &[Action::ShowContextMenu]);
}
