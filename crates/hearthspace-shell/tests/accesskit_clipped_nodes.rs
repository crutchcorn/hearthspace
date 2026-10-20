//! Scrolling an ancestor must register newly visible, otherwise unchanged nodes.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use accesskit_atspi_common::{
    Adapter, AdapterCallback, AppContext, Event, InterfaceSet, NodeId, NodeIdOrRoot, ObjectEvent,
    WindowBounds,
};
use masonry::accesskit::{
    Action, ActionHandler, ActionRequest, Affine, Node, NodeId as LocalId, Rect, Role, Tree,
    TreeId, TreeUpdate,
};

#[derive(Default)]
struct PlatformState {
    interfaces: HashMap<NodeId, u32>,
    errors: Vec<String>,
    children_events: Vec<(u64, u64, bool)>,
}

#[derive(Clone, Default)]
struct Platform(Arc<Mutex<PlatformState>>);

fn local(id: NodeId) -> u64 {
    (u128::from(id) >> 64) as u64
}

impl AdapterCallback for Platform {
    fn register_interfaces(&self, _: &Adapter, id: NodeId, interfaces: InterfaceSet) {
        let mut state = self.0.lock().unwrap();
        let current = state.interfaces.get(&id).copied().unwrap_or_default();
        if current & interfaces.bits() != 0 {
            state
                .errors
                .push(format!("duplicate interfaces for {id:?}"));
        }
        state.interfaces.insert(id, current | interfaces.bits());
    }

    fn unregister_interfaces(&self, _: &Adapter, id: NodeId, interfaces: InterfaceSet) {
        let mut state = self.0.lock().unwrap();
        let current = state.interfaces.get(&id).copied().unwrap_or_default();
        if current & interfaces.bits() != interfaces.bits() {
            state
                .errors
                .push(format!("removing absent interfaces for {id:?}"));
        }
        state.interfaces.insert(id, current & !interfaces.bits());
    }

    fn emit_event(&self, _: &Adapter, event: Event) {
        if let Event::Object {
            target: NodeIdOrRoot::Node(parent),
            event,
        } = event
        {
            let child = match event {
                ObjectEvent::ChildAdded(_, child) => Some((child, true)),
                ObjectEvent::ChildRemoved(child) => Some((child, false)),
                _ => None,
            };
            if let Some((child, added)) = child {
                let mut state = self.0.lock().unwrap();
                if added && state.interfaces.get(&child).copied().unwrap_or_default() == 0 {
                    state
                        .errors
                        .push(format!("ChildAdded before registration: {child:?}"));
                }
                state
                    .children_events
                    .push((local(parent), local(child), added));
            }
        }
    }
}

impl Platform {
    fn registered(&self, id: u64) -> bool {
        self.0
            .lock()
            .unwrap()
            .interfaces
            .iter()
            .any(|(node, interfaces)| local(*node) == id && *interfaces != 0)
    }

    fn assert_balanced(&self) {
        let state = self.0.lock().unwrap();
        assert!(state.errors.is_empty(), "{:?}", state.errors);
    }

    fn assert_child_event(&self, child: u64, added: bool) {
        assert!(
            self.0
                .lock()
                .unwrap()
                .children_events
                .contains(&(1, child, added)),
            "missing child event for node {child}, added={added}"
        );
    }
}

struct NoActions;
impl ActionHandler for NoActions {
    fn do_action(&mut self, _: ActionRequest) {}
}

fn content(offset: f64) -> Node {
    let mut node = Node::new(Role::GenericContainer);
    node.set_bounds(Rect::new(0.0, 0.0, 300.0, 360.0));
    node.set_transform(Affine::new([1.0, 0.0, 0.0, 1.0, 0.0, offset]));
    node.set_children((3..=11).map(LocalId).collect::<Vec<_>>());
    node
}

fn scroll_view(height: f64) -> Node {
    let mut node = Node::new(Role::ScrollView);
    node.set_bounds(Rect::new(0.0, 0.0, 300.0, height));
    node.set_clips_children();
    node.set_children([LocalId(2)]);
    node
}

fn initial_tree() -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    root.set_children([LocalId(1)]);
    let mut nodes = vec![
        (LocalId(0), root),
        (LocalId(1), scroll_view(120.0)),
        (LocalId(2), content(0.0)),
    ];
    for id in 3..=11 {
        let (role, name) = match id {
            8 => (Role::ComboBox, "Photo fit"),
            9 => (Role::Label, "Moves with the canvas."),
            _ => (Role::Label, "Other content"),
        };
        let y = (id - 3) as f64 * 40.0;
        let mut node = Node::new(role);
        node.set_label(name);
        node.set_bounds(Rect::new(0.0, y, 300.0, y + 30.0));
        if id == 8 {
            node.set_value("Repeat with canvas");
            node.add_action(Action::Focus);
            node.add_action(Action::Click);
        }
        nodes.push((LocalId(id), node));
    }
    TreeUpdate {
        tree_id: TreeId::ROOT,
        tree: Some(Tree::new(LocalId(0))),
        nodes,
        focus: LocalId(0),
    }
}

fn update(id: u64, node: Node) -> TreeUpdate {
    TreeUpdate {
        tree_id: TreeId::ROOT,
        tree: None,
        nodes: vec![(LocalId(id), node)],
        focus: LocalId(0),
    }
}

fn fixture() -> (Adapter, Platform) {
    let platform = Platform::default();
    let adapter = Adapter::new(
        &AppContext::new(Some("Clipped controls".into())),
        platform.clone(),
        initial_tree(),
        true,
        WindowBounds::default(),
        NoActions,
    );
    assert!(!platform.registered(8));
    assert!(!platform.registered(9));
    (adapter, platform)
}

fn assert_exposed_children_registered(adapter: &Adapter, platform: &Platform) {
    let root = adapter.platform_node(adapter.root_id());
    let scroll = adapter.platform_node(root.child_at_index(0).unwrap().unwrap());
    for index in 0..scroll.child_count().unwrap() as usize {
        let id = scroll.child_at_index(index).unwrap().unwrap();
        assert!(
            platform.registered(local(id)),
            "GetChildren must not expose an unregistered D-Bus object: {id:?}"
        );
    }
}

#[test]
fn scrolling_registers_unchanged_controls_and_labels_then_unregisters_them() {
    let (mut adapter, platform) = fixture();

    // Only the content ancestor changes, as in a Masonry Portal scroll.
    adapter.update(update(2, content(-180.0)));
    assert!(
        platform.registered(8),
        "the visible ComboBox needs its interfaces"
    );
    assert!(
        platform.registered(9),
        "ordinary text must be registered too"
    );
    assert_exposed_children_registered(&adapter, &platform);
    platform.assert_child_event(8, true);
    platform.assert_child_event(9, true);
    platform.assert_balanced();

    platform.0.lock().unwrap().children_events.clear();
    adapter.update(update(2, content(-180.0)));
    assert!(platform.0.lock().unwrap().children_events.is_empty());
    platform.assert_balanced();

    adapter.update(update(2, content(0.0)));
    assert!(!platform.registered(8));
    assert!(!platform.registered(9));
    platform.assert_child_event(8, false);
    platform.assert_child_event(9, false);
    assert_exposed_children_registered(&adapter, &platform);
    platform.assert_balanced();
    drop(adapter);
    platform.assert_balanced();
}

#[test]
fn resizing_the_clip_registers_descendants_without_child_updates() {
    let (mut adapter, platform) = fixture();
    adapter.update(update(1, scroll_view(300.0)));
    assert!(platform.registered(8));
    assert!(platform.registered(9));
    assert_exposed_children_registered(&adapter, &platform);
    platform.assert_balanced();
}

#[test]
fn moving_one_row_refreshes_its_unchanged_offscreen_neighbor() {
    let (mut adapter, platform) = fixture();
    // The filter includes one offscreen sibling of a visible node. Bringing
    // only the ComboBox into view must also register its unchanged next label.
    let mut combo = initial_tree()
        .nodes
        .into_iter()
        .find(|(id, _)| *id == LocalId(8))
        .unwrap()
        .1;
    combo.set_bounds(Rect::new(0.0, 20.0, 300.0, 50.0));
    adapter.update(update(8, combo));
    assert!(platform.registered(8));
    assert!(platform.registered(9));
    assert_exposed_children_registered(&adapter, &platform);
    platform.assert_balanced();
}

#[test]
fn simultaneous_ancestor_and_child_updates_register_and_notify_once() {
    let (mut adapter, platform) = fixture();
    let mut combo = initial_tree().nodes.remove(8).1;
    assert_eq!(combo.role(), Role::ComboBox);
    combo.set_value("Cover");
    let mut changes = update(2, content(-180.0));
    changes.nodes.push((LocalId(8), combo));
    adapter.update(changes);
    assert!(platform.registered(8));
    platform.assert_balanced();
    assert_eq!(
        platform
            .0
            .lock()
            .unwrap()
            .children_events
            .iter()
            .filter(|event| **event == (1, 8, true))
            .count(),
        1
    );
}

#[test]
fn reordering_children_refreshes_the_unchanged_neighbor_exception() {
    let (mut adapter, platform) = fixture();
    let mut reordered = content(0.0);
    reordered.set_children([3, 4, 5, 9, 6, 7, 8, 10, 11].map(LocalId));
    adapter.update(update(2, reordered));
    assert!(
        platform.registered(9),
        "the label now follows an onscreen sibling"
    );
    assert_exposed_children_registered(&adapter, &platform);
    platform.assert_child_event(9, true);
    platform.assert_balanced();
}

#[test]
fn moving_a_nested_clip_refreshes_outer_neighbors_and_transparent_descendants() {
    let platform = Platform::default();
    let mut tree = initial_tree();
    let mut nested = Node::new(Role::ScrollView);
    nested.set_transform(Affine::new([1.0, 0.0, 0.0, 1.0, 0.0, 200.0]));
    // Bounds are local to the nested viewport.
    nested.set_bounds(Rect::new(0.0, 0.0, 300.0, 30.0));
    nested.set_clips_children();
    nested.set_children([LocalId(12)]);
    tree.nodes[8].1 = nested.clone();
    let mut transparent = Node::new(Role::GenericContainer);
    transparent.set_children([LocalId(13)]);
    tree.nodes.push((LocalId(12), transparent));
    let mut child = Node::new(Role::Label);
    child.set_label("Nested content");
    child.set_bounds(Rect::new(0.0, 0.0, 100.0, 20.0));
    tree.nodes.push((LocalId(13), child.clone()));
    let mut adapter = Adapter::new(
        &AppContext::new(None),
        platform.clone(),
        tree,
        true,
        WindowBounds::default(),
        NoActions,
    );
    assert!(!platform.registered(13));
    child.set_role(Role::Button);
    child.add_action(Action::Click);
    adapter.update(update(13, child));
    assert!(
        !platform.registered(13),
        "a clipped descendant stays unregistered"
    );
    platform.assert_balanced();

    nested.set_transform(Affine::new([1.0, 0.0, 0.0, 1.0, 0.0, 20.0]));
    adapter.update(update(8, nested));
    assert!(platform.registered(13));
    assert!(
        platform.registered(9),
        "the outer next sibling becomes exposed"
    );
    assert_exposed_children_registered(&adapter, &platform);
    platform.assert_balanced();
}
