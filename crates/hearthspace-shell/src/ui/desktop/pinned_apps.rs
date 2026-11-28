//! Keep explicitly placed app shortcuts when upgrading, without populating the canvas.
use super::{Item, ShellState, context_items};
use hearthspace_common::frame_layout::{LayoutTarget, TargetKind};
use std::collections::{BTreeMap, BTreeSet};
use xilem::WindowId;

pub(in crate::ui) fn reconcile(state: &mut ShellState) {
    let mut ids = BTreeSet::new();
    for output in &state.frame_layout.snapshot.outputs {
        let targets = output
            .layout
            .placements
            .keys()
            .filter_map(|key| LayoutTarget::from_key(key))
            .chain(
                output
                    .layout
                    .groups
                    .values()
                    .flat_map(|group| group.members.iter().cloned()),
            );
        for target in targets {
            if target.kind == TargetKind::Item && target.id.starts_with("app:") {
                ids.insert(target.id);
            }
        }
    }
    let mut previous: BTreeMap<_, _> = std::mem::take(&mut state.desktop.pinned_apps)
        .into_iter()
        .map(|item| (item.content.id, item.window_id))
        .collect();
    let mut items = Vec::new();
    let capacity = hearthspace_common::MAX_DESKTOP_ITEMS.saturating_sub(state.desktop.items.len());
    for id in ids.into_iter().take(capacity) {
        if state.desktop.items.iter().any(|item| item.content.id == id) {
            continue;
        }
        let Some(content) = context_items::resolve(&id, &state.catalog, std::iter::empty()) else {
            continue;
        };
        items.push(Item {
            window_id: previous.remove(&id).unwrap_or_else(WindowId::next),
            content,
        });
    }
    state.desktop.pinned_apps = items;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::frame_layout::{self, Output, Snapshot};
    use hearthspace_common::frame_layout::{FrameEdge, ObjectPlacement};
    use hearthspace_desktop::app_catalog::AppCatalog;

    #[test]
    fn only_explicit_app_shortcuts_survive_upgrade_with_stable_surface_identity() {
        let directory = tempfile::tempdir().unwrap();
        let applications = directory.path().join("applications");
        std::fs::create_dir(&applications).unwrap();
        for id in ["editor", "terminal"] {
            std::fs::write(
                applications.join(format!("{id}.desktop")),
                format!("[Desktop Entry]\nType=Application\nName={id}\nExec=true\n"),
            )
            .unwrap();
        }
        let mut state = ShellState::new("/unused".into(), vec!["output".into()]);
        state.catalog = AppCatalog::load_from_data_dirs(vec![directory.path().into()]);
        let mut snapshot = Snapshot {
            version: 1,
            outputs: vec![Output {
                output: "output".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        frame_layout::apply(&mut state, snapshot.clone());
        assert!(state.desktop.pinned_apps.is_empty());
        assert!(state.desktop.items.is_empty());
        let target = LayoutTarget::new(TargetKind::Item, "app:editor.desktop");
        snapshot.outputs[0]
            .layout
            .set_placement(&target, ObjectPlacement::frame(FrameEdge::Left, 0.5));
        frame_layout::apply(&mut state, snapshot.clone());
        assert_eq!(state.desktop.pinned_apps.len(), 1);
        let identity = state.desktop.pinned_apps[0].window_id;
        reconcile(&mut state);
        assert_eq!(state.desktop.pinned_apps[0].window_id, identity);
        assert!(state.catalog.app_by_id("terminal.desktop").is_some());
        snapshot.outputs[0].layout.remove_target(&target);
        frame_layout::apply(&mut state, snapshot);
        assert!(state.desktop.pinned_apps.is_empty());
    }
}
