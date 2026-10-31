//! Exercise the action API used by the Unix AT-SPI bridge, including dispatch.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use accesskit_atspi_common::{
    Adapter, AdapterCallback, AppContext, Event, InterfaceSet, NodeId, PlatformNode, WindowBounds,
};
use masonry::accesskit::{
    Action, ActionHandler, ActionRequest, Node, NodeId as LocalId, Role, Tree, TreeId, TreeUpdate,
};

const WINDOW: LocalId = LocalId(1);
const TARGET: LocalId = LocalId(7);

#[path = "accesskit_context_menu/inherited_actions.rs"]
mod inherited_actions;

#[derive(Clone, Default)]
struct Registrations(Arc<Mutex<HashMap<NodeId, InterfaceSet>>>);

impl AdapterCallback for Registrations {
    fn register_interfaces(&self, _: &Adapter, id: NodeId, interfaces: InterfaceSet) {
        let mut registered = self.0.lock().unwrap();
        let current = registered.entry(id).or_default();
        assert_eq!(*current & interfaces, InterfaceSet::empty());
        *current = *current | interfaces;
    }

    fn unregister_interfaces(&self, _: &Adapter, id: NodeId, interfaces: InterfaceSet) {
        let mut registered = self.0.lock().unwrap();
        let current = registered.get_mut(&id).unwrap();
        assert_eq!(*current & interfaces, interfaces);
        *current = *current ^ interfaces;
    }

    fn emit_event(&self, _: &Adapter, _: Event) {}
}

#[derive(Clone, Default)]
struct Requests(Arc<Mutex<Vec<ActionRequest>>>);

impl ActionHandler for Requests {
    fn do_action(&mut self, request: ActionRequest) {
        self.0.lock().unwrap().push(request);
    }
}

impl Requests {
    fn assert_received(&self, tree: TreeId, node: LocalId, actions: &[Action]) {
        let requests = std::mem::take(&mut *self.0.lock().unwrap());
        let expected: Vec<_> = actions
            .iter()
            .map(|action| ActionRequest {
                action: *action,
                target_tree: tree,
                target_node: node,
                data: None,
            })
            .collect();
        assert_eq!(requests, expected);
    }
}

fn button(actions: &[Action]) -> Node {
    let mut node = Node::new(Role::Button);
    node.set_label("Application");
    for action in actions {
        node.add_action(*action);
    }
    node
}

struct Fixture {
    adapter: Adapter,
    registrations: Registrations,
    requests: Requests,
}

impl Fixture {
    fn new(target: Node) -> Self {
        Self::with_root(Node::new(Role::Window), target)
    }

    fn with_root(mut root: Node, target: Node) -> Self {
        root.set_children([TARGET]);
        let registrations = Registrations::default();
        let requests = Requests::default();
        let adapter = Adapter::new(
            &AppContext::new(Some("Context menu actions".into())),
            registrations.clone(),
            TreeUpdate {
                tree_id: TreeId::ROOT,
                tree: Some(Tree::new(WINDOW)),
                nodes: vec![(WINDOW, root), (TARGET, target)],
                focus: WINDOW,
            },
            true,
            WindowBounds::default(),
            requests.clone(),
        );
        Self {
            adapter,
            registrations,
            requests,
        }
    }

    fn target(&self) -> PlatformNode {
        let root = self.adapter.platform_node(self.adapter.root_id());
        self.adapter
            .platform_node(root.child_at_index(0).unwrap().unwrap())
    }

    fn update(&mut self, id: LocalId, node: Node) {
        self.adapter.update(TreeUpdate {
            tree_id: TreeId::ROOT,
            tree: None,
            nodes: vec![(id, node)],
            focus: WINDOW,
        });
    }

    fn assert_actions(&self, node: &PlatformNode, names: &[&str]) {
        assert_eq!(node.n_actions().unwrap(), names.len() as i32);
        assert_eq!(node.supports_action().unwrap(), !names.is_empty());
        let actions = node.actions().unwrap();
        let actual_names: Vec<_> = actions
            .iter()
            .map(|action| action.localized_name.as_str())
            .collect();
        assert_eq!(actual_names, names);
        for (index, action) in actions.iter().enumerate() {
            assert_eq!(node.action_name(index as i32).unwrap(), names[index]);
            assert!(action.description.is_empty());
            assert!(action.key_binding.is_empty());
        }

        // InterfaceSet is public, but its Interface enum is not re-exported.
        let action_interface: InterfaceSet =
            serde_json::from_str(r#"["org.a11y.atspi.Action"]"#).unwrap();
        let interfaces = node.interfaces().unwrap();
        assert_eq!(
            interfaces & action_interface,
            if names.is_empty() {
                InterfaceSet::empty()
            } else {
                action_interface
            }
        );
        assert_eq!(
            self.registrations.0.lock().unwrap().get(&node.id()),
            Some(&interfaces)
        );
    }

    fn assert_invalid_indices(&self, node: &PlatformNode, count: i32) {
        for index in [i32::MIN, -1, count, count + 1, i32::MAX] {
            assert_eq!(node.action_name(index).unwrap(), "", "index {index}");
            assert!(!node.do_action(index).unwrap(), "index {index}");
        }
        self.requests.assert_received(TreeId::ROOT, TARGET, &[]);
    }
}

#[test]
fn click_and_context_menu_keep_default_click_first() {
    let fixture = Fixture::new(button(&[Action::ShowContextMenu, Action::Click]));
    let node = fixture.target();
    fixture.assert_actions(&node, &["click", "showContextMenu"]);
    assert!(node.do_action(0).unwrap());
    assert!(node.do_action(1).unwrap());
    fixture.requests.assert_received(
        TreeId::ROOT,
        TARGET,
        &[Action::Click, Action::ShowContextMenu],
    );
    fixture.assert_invalid_indices(&node, 2);
}

#[test]
fn context_menu_only_exposes_action_interface_and_dispatches_index_zero() {
    let fixture = Fixture::new(button(&[Action::ShowContextMenu]));
    let node = fixture.target();
    fixture.assert_actions(&node, &["showContextMenu"]);
    assert!(node.do_action(0).unwrap());
    fixture
        .requests
        .assert_received(TreeId::ROOT, TARGET, &[Action::ShowContextMenu]);
    fixture.assert_invalid_indices(&node, 1);
}

#[test]
fn click_only_preserves_default_action() {
    let fixture = Fixture::new(button(&[Action::Click]));
    let node = fixture.target();
    fixture.assert_actions(&node, &["click"]);
    assert!(node.do_action(0).unwrap());
    fixture
        .requests
        .assert_received(TreeId::ROOT, TARGET, &[Action::Click]);
    fixture.assert_invalid_indices(&node, 1);
}

#[test]
fn absent_or_unmapped_actions_do_not_expose_or_dispatch_an_action() {
    let mut fixture = Fixture::new(button(&[]));
    let node = fixture.target();
    fixture.assert_actions(&node, &[]);
    fixture.assert_invalid_indices(&node, 0);

    fixture.update(TARGET, button(&[Action::Focus]));
    fixture.assert_actions(&node, &[]);
    fixture.assert_invalid_indices(&node, 0);
}

#[test]
fn removing_context_menu_rejects_its_old_index_and_retains_click() {
    let mut fixture = Fixture::new(button(&[Action::Click, Action::ShowContextMenu]));
    let node = fixture.target();
    fixture.assert_actions(&node, &["click", "showContextMenu"]);

    fixture.update(TARGET, button(&[Action::Click]));
    fixture.assert_actions(&node, &["click"]);
    fixture.assert_invalid_indices(&node, 1);
    assert!(node.do_action(0).unwrap());
    fixture
        .requests
        .assert_received(TreeId::ROOT, TARGET, &[Action::Click]);
}

#[test]
fn removing_click_reindexes_context_menu_and_last_action_updates_interfaces() {
    let mut fixture = Fixture::new(button(&[Action::Click, Action::ShowContextMenu]));
    let node = fixture.target();
    fixture.assert_actions(&node, &["click", "showContextMenu"]);

    fixture.update(TARGET, button(&[Action::ShowContextMenu]));
    fixture.assert_actions(&node, &["showContextMenu"]);
    fixture.assert_invalid_indices(&node, 1);
    assert!(node.do_action(0).unwrap());
    fixture
        .requests
        .assert_received(TreeId::ROOT, TARGET, &[Action::ShowContextMenu]);

    fixture.update(TARGET, button(&[]));
    fixture.assert_actions(&node, &[]);
    fixture.assert_invalid_indices(&node, 0);

    fixture.update(TARGET, button(&[Action::ShowContextMenu]));
    fixture.assert_actions(&node, &["showContextMenu"]);
    assert!(node.do_action(0).unwrap());
    fixture
        .requests
        .assert_received(TreeId::ROOT, TARGET, &[Action::ShowContextMenu]);
}

#[test]
fn parent_advertised_child_actions_keep_the_child_as_request_target() {
    let mut root = Node::new(Role::Window);
    root.add_child_action(Action::Click);
    root.add_child_action(Action::ShowContextMenu);
    let fixture = Fixture::with_root(root, button(&[]));
    let node = fixture.target();
    fixture.assert_actions(&node, &["click", "showContextMenu"]);
    assert!(node.do_action(0).unwrap());
    assert!(node.do_action(1).unwrap());
    fixture.requests.assert_received(
        TreeId::ROOT,
        TARGET,
        &[Action::Click, Action::ShowContextMenu],
    );
}

#[test]
fn grafted_tree_actions_preserve_target_tree_and_local_node_id() {
    let subtree = TreeId("d4e63923-b575-4ace-8daf-c04d78df30d1".parse().unwrap());
    let mut graft = Node::new(Role::GenericContainer);
    graft.set_tree_id(subtree);
    let mut fixture = Fixture::new(graft);
    fixture.adapter.update(TreeUpdate {
        tree_id: subtree,
        tree: Some(Tree::new(TARGET)),
        // Deliberately reuse the graft's local ID in a different tree.
        nodes: vec![(TARGET, button(&[Action::Click, Action::ShowContextMenu]))],
        focus: TARGET,
    });
    let node = fixture.target();
    fixture.assert_actions(&node, &["click", "showContextMenu"]);
    assert!(node.do_action(1).unwrap());
    assert!(node.do_action(0).unwrap());
    fixture
        .requests
        .assert_received(subtree, TARGET, &[Action::ShowContextMenu, Action::Click]);
}
