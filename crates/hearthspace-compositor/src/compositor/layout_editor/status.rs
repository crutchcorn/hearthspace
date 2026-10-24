use hearthspace_ipc::frame_layout::{Group, Output, PinnedWindow, Snapshot, Widget};

use super::{App, LayoutTarget, ObjectPlacement, Target, TargetKind};

impl App {
    pub(super) fn layout_status(&self, available: bool) -> Snapshot {
        let state = &self.layout_editor;
        let mut snapshot = Snapshot {
            version: 1,
            revision: state.revision,
            popup_epoch: state.popup_epoch,
            allow_hide_tools: state.allow_hide_tools,
            confirmation_max_width: 420.,
            confirmation_max_height: 452.,
            editor_max_width: 260.,
            editor_max_height: 452.,
            ..Snapshot::default()
        };
        if !available {
            return snapshot;
        }
        snapshot.animating = self.layout_animating();
        snapshot.outputs = state
            .layouts
            .iter()
            .filter_map(|(output, layout)| {
                let view = self.frame_output_rect(output)?;
                let widgets = hearthspace_common::frame_layout::WIDGETS
                    .iter()
                    .map(|id| {
                        let target = Target {
                            output: output.clone(),
                            target: LayoutTarget::widget(*id),
                        };
                        let placed = self.frame_placed(&target);
                        Widget {
                            id: (*id).into(),
                            merged: self.frame_widget_merged(output, id),
                            group: placed.and_then(|placed| placed.group.clone()),
                            orientation: orientation(placed.is_some_and(|placed| placed.vertical)),
                            rect: placed.map_or(Default::default(), |placed| placed.rect.into()),
                            expanded: layout.is_expanded(id),
                            hidden: layout.placement(&target.target) == ObjectPlacement::Hidden,
                        }
                    })
                    .collect();
                let groups = state
                    .placed
                    .iter()
                    .filter(|placed| {
                        placed.target.output == *output
                            && placed.target.target.kind == TargetKind::Group
                    })
                    .map(|placed| Group {
                        id: placed.target.target.id.clone(),
                        material: self.frame_group_material(output, &placed.target.target.id),
                        backing_only: super::compact::backing(&placed.target.target.id),
                        hovered: self.frame_group_hovered(output, &placed.target.target.id),
                        focused: self.frame_group_focused(output, &placed.target.target.id),
                        rect: placed.rect.into(),
                        grip: self.layout_grip_rect(placed).into(),
                        scale: if placed.placement.is_frame() {
                            1.
                        } else {
                            self.viewport_scale
                        },
                        orientation: orientation(placed.vertical),
                    })
                    .collect();
                Some(Output {
                    output: output.clone(),
                    rect: view.into(),
                    compact: view.width <= 760.,
                    layout: self.live_frame_layout(layout),
                    widgets,
                    groups,
                })
            })
            .collect();
        snapshot.pinned_windows = self
            .windows
            .iter()
            .filter_map(|window| {
                let placed = self.pinned_window_target(window.id)?;
                Some(PinnedWindow {
                    id: window.id,
                    title: super::window_title(&window.surface).unwrap_or_default(),
                    app_id: super::window_app_id(&window.surface).unwrap_or_default(),
                    edge: placed.placement.edge(),
                    output: placed.target.output.clone(),
                    rect: self.pinned_reveal_rect(placed).into(),
                    expanded: state.expanded_window == Some(window.id),
                    hovered: state.hovered_window == Some(window.id),
                })
            })
            .collect();
        let session = state.session.as_ref();
        let selected = session.and_then(|session| session.selected.as_ref());
        let view = session
            .and_then(|session| self.frame_output_rect(&session.output))
            .unwrap_or_default();
        snapshot.editing = session.is_some_and(|session| session.editing);
        snapshot.session = session.map_or(0, |session| session.id);
        snapshot.output = session.map(|session| session.output.clone());
        snapshot.selected = selected.cloned();
        snapshot.selected_title =
            selected.map_or_else(String::new, |target| self.target_title(target));
        snapshot.confirmation = session.and_then(|session| session.confirmation);
        snapshot.focus_request = state.focus_request.clone();
        snapshot.confirmation_max_width = (view.width - 48.).clamp(1., 420.);
        snapshot.confirmation_max_height = (view.height - 48.).max(1.);
        snapshot.editor_max_width = (view.width - 24.).clamp(120., 260.);
        snapshot.editor_max_height =
            (view.height - if view.width <= 760. { 156. } else { 180. }).max(54.);
        snapshot
    }
}

fn orientation(vertical: bool) -> String {
    if vertical { "vertical" } else { "horizontal" }.into()
}
