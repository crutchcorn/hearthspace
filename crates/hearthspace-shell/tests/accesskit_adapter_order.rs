//! Wayland may redraw the four corners in a different order from creation.
//! AccessKit must retain all windows when lazy accessibility activates this way.
use std::sync::{Arc, RwLock};

use accesskit_atspi_common::{
    Adapter, AdapterCallback, AppContext, Event, InterfaceSet, NodeId, PlatformRoot, WindowBounds,
};
use masonry::accesskit::{self, ActionHandler, ActionRequest, Node, Role, Tree, TreeUpdate};

struct NoActions;
impl ActionHandler for NoActions {
    fn do_action(&mut self, _: ActionRequest) {}
}

struct NoPlatform;
impl AdapterCallback for NoPlatform {
    fn register_interfaces(&self, _: &Adapter, _: NodeId, _: InterfaceSet) {}
    fn unregister_interfaces(&self, _: &Adapter, _: NodeId, _: InterfaceSet) {}
    fn emit_event(&self, _: &Adapter, _: Event) {}
}

fn tree(label: &str) -> TreeUpdate {
    let id = accesskit::NodeId(1);
    let mut node = Node::new(Role::Window);
    node.set_label(label);
    TreeUpdate {
        tree_id: accesskit::TreeId::ROOT,
        nodes: vec![(id, node)],
        tree: Some(Tree::new(id)),
        focus: id,
    }
}

fn adapter(context: &Arc<RwLock<AppContext>>, id: usize) -> Adapter {
    Adapter::with_id(
        id,
        context,
        NoPlatform,
        tree(&format!("Corner {id}")),
        false,
        WindowBounds::default(),
        NoActions,
    )
}

fn window_ids(root: &PlatformRoot) -> Vec<usize> {
    root.map_child_ids(|(id, _)| id).unwrap()
}

#[test]
fn out_of_order_corner_activation_supports_update_removal_and_reactivation() {
    let context = AppContext::new(Some("Hearthspace shell".into()));
    let root = PlatformRoot::new(&context);
    let last = adapter(&context, 2);
    let mut first = adapter(&context, 0);
    let middle = adapter(&context, 1);
    assert_eq!(window_ids(&root), [0, 1, 2]);
    assert_eq!(
        root.child_at_index(2).unwrap().unwrap().name().unwrap(),
        "Corner 2"
    );

    first.update(tree("Notifications"));
    assert_eq!(
        first.platform_node(first.root_id()).name().unwrap(),
        "Notifications"
    );
    drop(middle);
    assert_eq!(window_ids(&root), [0, 2]);
    assert_eq!(
        root.child_at_index(1).unwrap().unwrap().name().unwrap(),
        "Corner 2"
    );

    let middle = adapter(&context, 1);
    assert_eq!(window_ids(&root), [0, 1, 2]);
    drop(first);
    drop(last);
    assert_eq!(window_ids(&root), [1]);
    drop(middle);
    assert_eq!(root.child_count().unwrap(), 0);
}
