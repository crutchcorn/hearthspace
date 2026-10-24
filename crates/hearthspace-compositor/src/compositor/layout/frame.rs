//! Canonical frame preferences use app slots; live IDs are only presentation bindings.
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

fn runtime_targets(layout: &FrameLayout) -> Vec<LayoutTarget> {
    layout
        .placements
        .keys()
        .filter_map(|key| LayoutTarget::from_key(key))
        .chain(
            layout
                .groups
                .values()
                .flat_map(|group| group.members.clone()),
        )
        .filter(|target| target.kind == TargetKind::Window && target.id.parse::<u64>().is_ok())
        .collect()
}

impl LayoutStore {
    pub(in crate::compositor) fn frame_layouts(&self) -> BTreeMap<String, FrameLayout> {
        // Numeric window references from older files cannot identify a client in
        // this process. Never let accidental runtime ID reuse bind them.
        let mut layouts = self.layout.frames.clone();
        for layout in layouts.values_mut() {
            for target in runtime_targets(layout) {
                layout.remove_target(&target);
            }
        }
        layouts
    }

    pub(in crate::compositor) fn allow_hide_tools(&self) -> bool {
        self.layout.allow_hide_tools
    }

    fn canonical_target(&self, target: &LayoutTarget) -> LayoutTarget {
        if target.kind == TargetKind::Window
            && let Ok(id) = target.id.parse::<u64>()
            && let Some((app_id, slot)) = self.claimed.get(&id)
        {
            return LayoutTarget::new(TargetKind::Window, saved_id(app_id, *slot));
        }
        target.clone()
    }

    fn runtime_id(&self, target: &LayoutTarget) -> Option<u64> {
        if target.kind != TargetKind::Window {
            return None;
        }
        target.id.parse().ok().or_else(|| {
            self.claimed.iter().find_map(|(id, (app_id, slot))| {
                (saved_id(app_id, *slot) == target.id).then_some(*id)
            })
        })
    }
}

impl App {
    /// Resolve a command's live target before reading or mutating preferences.
    /// Unclaimed windows (including transient dialogs) remain process-local.
    pub(in crate::compositor) fn canonical_layout_target(
        &self,
        target: &LayoutTarget,
    ) -> LayoutTarget {
        self.layout_store.canonical_target(target)
    }

    /// Only mapped, navigable clients participate in frame presentation. A saved
    /// slot with no live binding stays in the editor but has no visible target.
    pub(in crate::compositor) fn live_layout_target(
        &self,
        target: &LayoutTarget,
    ) -> Option<LayoutTarget> {
        if target.kind != TargetKind::Window {
            return Some(target.clone());
        }
        let id = self.layout_store.runtime_id(target)?;
        self.windows
            .iter()
            .enumerate()
            .any(|(index, window)| {
                window.id == id && window.mapped && self.is_navigable_window(index)
            })
            .then(|| LayoutTarget::new(TargetKind::Window, id.to_string()))
    }

    /// Shell status uses runtime IDs, matching commands and the geometry projection.
    pub(in crate::compositor) fn live_frame_layout(&self, layout: &FrameLayout) -> FrameLayout {
        let mut live = layout.clone();
        for (id, (app_id, slot)) in &self.layout_store.claimed {
            rename_window(&mut live, &saved_id(app_id, *slot), &id.to_string());
        }
        // Keep unavailable slots in this metadata so groups remain valid. Only
        // frame_placements supplies visible objects; it filters absent members.
        live
    }

    pub(in crate::compositor) fn restore_window_frame_placement(&mut self, id: u64) {
        if !self.layout_store.claimed.contains_key(&id) {
            return;
        }
        // The binding changes what is visible, never the saved preference.
        self.reconcile_layout_editor();
    }

    /// Call when a window record is removed, not on a reversible Wayland unmap.
    /// Explicit confirmed close separately removes the canonical preference.
    pub(in crate::compositor) fn release_window_layout_binding(&mut self, id: u64) {
        self.layout_store.claimed.remove(&id);
        let transient = LayoutTarget::new(TargetKind::Window, id.to_string());
        for layout in self.layout_editor.layouts.values_mut() {
            layout.remove_target(&transient);
        }
    }

    pub(super) fn remember_frame_layout(&mut self) {
        let mut layouts = self.layout_editor.layouts.clone();
        for layout in layouts.values_mut() {
            for target in runtime_targets(layout) {
                layout.remove_target(&target);
            }
        }
        self.layout_store.layout.frames = layouts;
        self.layout_store.layout.allow_hide_tools = self.layout_editor.allow_hide_tools;
    }
}

#[cfg(test)]
#[path = "frame_tests.rs"]
mod tests;
