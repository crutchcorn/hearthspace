// Copyright 2026 The Hearthspace Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

use super::{AdapterChangeHandler, FilterResult, Node, TreeChangeHandler, filter};

// A transparent generic container returns ExcludeNode before checking its
// ancestors. Its descendants can therefore pass common_filter individually
// while remaining unreachable through the exposed tree.
pub(super) fn effective_filter(node: &Node<'_>) -> FilterResult {
    let result = filter(node);
    if result == FilterResult::ExcludeSubtree {
        return result;
    }
    let mut parent = node.parent();
    while let Some(ancestor) = parent {
        if filter(&ancestor) == FilterResult::ExcludeSubtree {
            return FilterResult::ExcludeSubtree;
        }
        parent = ancestor.parent();
    }
    result
}

impl AdapterChangeHandler<'_> {
    pub(super) fn refresh_filter_scopes(&mut self, old: &Node<'_>, new: &Node<'_>) {
        if old.raw_bounds() == new.raw_bounds()
            && old.direct_transform() == new.direct_transform()
            && old.clips_children() == new.clips_children()
            && old.is_hidden() == new.is_hidden()
            && old.role() == new.role()
            && old.parent_id() == new.parent_id()
            && old.child_ids().eq(new.child_ids())
        {
            return;
        }

        // Clipping also depends on the preceding/following visible sibling.
        // Start above the changed node even when that node itself clips.
        fn scope<'a>(node: &Node<'a>) -> Node<'a> {
            let mut result = *node;
            while let Some(parent) = result.parent() {
                result = parent;
                if parent.clips_children() {
                    break;
                }
            }
            result
        }

        for root in [scope(old), scope(new)] {
            if !self.refreshed_filter_scopes.insert(root.id()) {
                continue;
            }
            // Traverse raw children, including nodes that were clipped before.
            // Postorder registers children before their parent's ChildAdded.
            let mut pending = vec![(root, false)];
            while let Some((node, visited)) = pending.pop() {
                if !visited {
                    pending.push((node, true));
                    pending.extend(node.children().map(|child| (child, false)));
                } else if let (Some(old_node), Some(new_node)) = (
                    old.tree_state.node_by_id(node.id()),
                    new.tree_state.node_by_id(node.id()),
                ) {
                    self.node_updated(&old_node, &new_node);
                }
            }
        }
    }
}
