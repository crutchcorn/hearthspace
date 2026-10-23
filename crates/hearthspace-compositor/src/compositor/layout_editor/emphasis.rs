//! Group materials follow hover and native focus across their separate child surfaces.
use super::{App, Target, TargetKind, window_app_id};
use crate::compositor::HitTarget;
use hearthspace_common::frame_layout::pinned_window_reveal_from_app_id;
use smithay::desktop::find_popup_root_surface;

type Group = (String, String);

#[derive(Default, PartialEq)]
pub(super) struct State {
    hovered: Option<Group>,
    focused: Option<Group>,
    hovered_bar: Option<Group>,
    focused_bar: Option<Group>,
}

impl App {
    pub(in crate::compositor) fn update_layout_emphasis(&mut self) {
        let next = if self.session_locked()
            || self.canvas_preview_active()
            || self.layout_confirmation_active()
        {
            State::default()
        } else {
            let hovered = self.hit_test(self.pointer_location).map(|hit| match hit {
                HitTarget::Client { window_index, .. }
                | HitTarget::TitleBar { window_index }
                | HitTarget::TitlebarButton { window_index, .. }
                | HitTarget::ResizeBorder { window_index, .. } => window_index,
            });
            let focused = self.keyboard.current_focus().and_then(|focus| {
                self.window_index_for_surface(focus.surface()).or_else(|| {
                    self.popups
                        .find_popup(focus.surface())
                        .and_then(|popup| find_popup_root_surface(&popup).ok())
                        .and_then(|root| self.window_index_for_surface(&root))
                })
            });
            State {
                hovered: hovered.and_then(|index| self.emphasized_group_for_window(index)),
                focused: focused.and_then(|index| self.emphasized_group_for_window(index)),
                hovered_bar: hovered.and_then(|index| self.compact_bar_for_window(index)),
                focused_bar: focused.and_then(|index| self.compact_bar_for_window(index)),
            }
        };
        if self.layout_editor.emphasis != next {
            self.layout_editor.emphasis = next;
            self.layout_changed();
        }
    }

    fn emphasized_group_for_window(&self, index: usize) -> Option<Group> {
        if !self.windows[index].mapped {
            return None;
        }
        let target = self.frame_target_for_window(index).or_else(|| {
            let id = window_app_id(&self.windows[index].surface)?;
            let id = pinned_window_reveal_from_app_id(&id)?;
            Some(self.pinned_window_target(id)?.target.clone())
        })?;
        self.emphasized_group_for_target(&target)
    }

    fn emphasized_group_for_target(&self, target: &Target) -> Option<Group> {
        let placed = self.frame_placed(target)?;
        let group = if target.target.kind == TargetKind::Group {
            &target.target.id
        } else {
            placed.group.as_ref()?
        };
        Some((target.output.clone(), group.clone()))
    }

    pub(super) fn frame_group_hovered(&self, output: &str, id: &str) -> bool {
        [
            &self.layout_editor.emphasis.hovered,
            &self.layout_editor.emphasis.hovered_bar,
        ]
        .into_iter()
        .flatten()
        .any(|(name, group)| name == output && group == id)
    }

    pub(super) fn frame_group_focused(&self, output: &str, id: &str) -> bool {
        [
            &self.layout_editor.emphasis.focused,
            &self.layout_editor.emphasis.focused_bar,
        ]
        .into_iter()
        .flatten()
        .any(|(name, group)| name == output && group == id)
    }

    pub(in crate::compositor) fn frame_group_emphasized(&self, index: usize) -> bool {
        self.frame_group_identity(index)
            .is_some_and(|(output, id)| {
                self.frame_group_hovered(&output, &id) || self.frame_group_focused(&output, &id)
            })
    }
}
