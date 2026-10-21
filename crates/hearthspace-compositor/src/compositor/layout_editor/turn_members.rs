//! A turn owns every mapped surface belonging to one visible toolbar composition.
use super::{App, Target, TargetKind};

impl App {
    pub(in crate::compositor::layout_editor) fn layout_turn_members(
        &self,
        target: &Target,
    ) -> Vec<u64> {
        self.windows
            .iter()
            .enumerate()
            .filter_map(|(index, window)| {
                if !window.mapped {
                    return None;
                }
                let member = self.frame_target_for_window(index)?;
                if member.output != target.output {
                    return None;
                }
                let placed = self.frame_placed(&member)?;
                // Group backing and external grip share the group target. Enumerating
                // surfaces, rather than targets, keeps both in the same composition.
                (member == *target
                    || target.target.kind == TargetKind::Group
                        && placed.group.as_deref() == Some(target.target.id.as_str()))
                .then_some(window.id)
            })
            .collect()
    }
}
