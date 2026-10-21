//! Saved frame windows use the existing per-application slot, never process window IDs.
use super::*;
use hearthspace_common::frame_layout::{FrameLayout, LayoutTarget, TargetKind};
use std::collections::BTreeMap;

fn saved_id(app_id: &str, slot: u8) -> String {
    let mut id = String::from("saved-");
    for byte in app_id.bytes() {
        use std::fmt::Write;
        let _ = write!(id, "{byte:02x}");
    }
    format!("{id}-{slot}")
}
fn rename_window(layout: &mut FrameLayout, from: &str, to: &str) {
    let old = LayoutTarget::new(TargetKind::Window, from).key();
    if let Some(placement) = layout.placements.remove(&old) {
        layout
            .placements
            .insert(LayoutTarget::new(TargetKind::Window, to).key(), placement);
    }
    for group in layout.groups.values_mut() {
        for member in &mut group.members {
            if member.kind == TargetKind::Window && member.id == from {
                member.id = to.into();
            }
        }
    }
}
impl LayoutStore {
    pub(in crate::compositor) fn frame_layouts(&self) -> BTreeMap<String, FrameLayout> {
        self.layout.frames.clone()
    }
    pub(in crate::compositor) fn allow_hide_tools(&self) -> bool {
        self.layout.allow_hide_tools
    }
}
impl App {
    pub(in crate::compositor) fn restore_window_frame_placement(&mut self, id: u64) {
        let Some((app_id, slot)) = self.layout_store.claimed.get(&id) else {
            return;
        };
        let old = saved_id(app_id, *slot);
        for layout in self.layout_editor.layouts.values_mut() {
            rename_window(layout, &old, &id.to_string());
        }
        self.reconcile_layout_editor();
    }
    pub(super) fn remember_frame_layout(&mut self) {
        let mut layouts = self.layout_editor.layouts.clone();
        for (id, (app_id, slot)) in &self.layout_store.claimed {
            let saved = saved_id(app_id, *slot);
            for layout in layouts.values_mut() {
                rename_window(layout, &id.to_string(), &saved);
            }
        }
        // Unclaimed transient windows cannot be meaningfully reopened in a later process.
        for layout in layouts.values_mut() {
            let transient: Vec<_> = layout
                .placements
                .keys()
                .filter_map(|key| LayoutTarget::from_key(key))
                .chain(layout.groups.values().flat_map(|g| g.members.clone()))
                .filter(|t| t.kind == TargetKind::Window && t.id.parse::<u64>().is_ok())
                .collect();
            for target in transient {
                layout.remove_target(&target);
            }
        }
        self.layout_store.layout.frames = layouts;
        self.layout_store.layout.allow_hide_tools = self.layout_editor.allow_hide_tools;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hearthspace_common::frame_layout::{FrameEdge, ObjectPlacement};
    #[test]
    fn window_slot_identity_restores_group_members_without_runtime_id_reuse() {
        let mut layout = FrameLayout::default();
        let window = LayoutTarget::new(TargetKind::Window, "73");
        let clock = LayoutTarget::widget("clock");
        layout.set_placement(&window, ObjectPlacement::frame(FrameEdge::Bottom, 0.5));
        layout.merge(&window, &clock, false).unwrap();
        let saved = saved_id("org.example.App", 2);
        rename_window(&mut layout, "73", &saved);
        let bytes = serde_json::to_vec(&layout).unwrap();
        let mut restored: FrameLayout = serde_json::from_slice(&bytes).unwrap();
        assert!(
            restored
                .group_for(&LayoutTarget::new(TargetKind::Window, "73"))
                .is_none()
        );
        rename_window(&mut restored, &saved, "902");
        let new = LayoutTarget::new(TargetKind::Window, "902");
        assert!(restored.group_for(&new).is_some());
        assert_eq!(
            restored.placement(&new),
            layout.placement(&LayoutTarget::new(TargetKind::Window, saved))
        );
    }
}
