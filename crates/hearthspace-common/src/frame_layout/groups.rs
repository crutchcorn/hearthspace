use super::{FrameGroup, FrameLayout, LayoutTarget, ObjectPlacement, TargetKind};

impl FrameLayout {
    /// Detach a member or dissolve an entire group, retaining its current placement.
    pub fn ungroup(&mut self, target: &LayoutTarget) {
        let Some(group) = self.group_for(target).cloned() else {
            return;
        };
        let placement = self.placement(target);
        let (detached, remaining): (Vec<_>, Vec<_>) = group
            .members
            .into_iter()
            .partition(|member| target.kind == TargetKind::Group || member == target);
        for member in detached {
            self.placements.insert(member.key(), placement);
        }
        if remaining.len() < 2 {
            for member in remaining {
                self.placements.insert(member.key(), placement);
            }
            self.groups.remove(&group.id);
            self.placements.remove(&format!("group:{}", group.id));
        } else {
            self.groups.insert(
                group.id.clone(),
                FrameGroup {
                    id: group.id,
                    members: remaining,
                },
            );
        }
    }

    /// Placement changes always separate a member first. Removing a group hides its widgets
    /// and returns non-widget members to their ordinary canvas/background layer.
    pub fn set_placement(&mut self, target: &LayoutTarget, mut placement: ObjectPlacement) {
        if !target.valid() || !placement.valid() {
            return;
        }
        if placement == ObjectPlacement::Hidden {
            if target.kind == TargetKind::Group {
                let Some(group) = self.groups.get(&target.id).cloned() else {
                    return;
                };
                self.ungroup(target);
                for member in group.members {
                    let placement = if member.kind == TargetKind::Widget {
                        ObjectPlacement::Hidden
                    } else {
                        super::default_placement(&member)
                    };
                    self.set_placement(&member, placement);
                }
                return;
            }
            if target.kind != TargetKind::Widget || (target.id == "tools" && !self.allow_hide_tools)
            {
                return;
            }
        }
        if target.kind == TargetKind::Group && !self.groups.contains_key(&target.id) {
            return;
        }
        let previous = self.placement(target);
        if target.kind != TargetKind::Group {
            self.ungroup(target);
        }
        // Coordinate-only drag updates must not discard the widget's measured size.
        if target.kind == TargetKind::Widget {
            match (&mut placement, previous) {
                (
                    ObjectPlacement::Canvas { width, height, .. },
                    ObjectPlacement::Canvas {
                        width: old_width,
                        height: old_height,
                        ..
                    },
                )
                | (
                    ObjectPlacement::Background { width, height, .. },
                    ObjectPlacement::Background {
                        width: old_width,
                        height: old_height,
                        ..
                    },
                ) => {
                    *width = width.or(old_width);
                    *height = height.or(old_height);
                }
                _ => {}
            }
        }
        self.placements.insert(target.key(), placement);
    }

    /// Merge at a frame destination, preserving member order and flattening source groups.
    pub fn merge(
        &mut self,
        source: &LayoutTarget,
        target: &LayoutTarget,
        before: bool,
    ) -> Option<LayoutTarget> {
        if !source.valid() || !target.valid() || source == target {
            return None;
        }
        let placement = self.placement(target);
        if !placement.is_frame() {
            return None;
        }
        let source_members = if source.kind == TargetKind::Group {
            self.groups.get(&source.id)?.members.clone()
        } else {
            vec![source.clone()]
        };
        let target_group = self.group_for(target).cloned();
        if source.kind == TargetKind::Group
            && target_group.as_ref().is_some_and(|g| g.id == source.id)
        {
            return None;
        }
        let mut members = target_group
            .as_ref()
            .map(|group| group.members.clone())
            .unwrap_or_else(|| vec![target.clone()]);
        members.retain(|member| !source_members.contains(member));
        if members.is_empty() {
            return None;
        }
        let index = if target.kind == TargetKind::Group {
            if before { 0 } else { members.len() }
        } else {
            members
                .iter()
                .position(|member| member == target)
                .map(|i| i + usize::from(!before))?
        };
        for member in &source_members {
            self.ungroup(member);
        }
        members.splice(index..index, source_members);
        let id = target_group.map(|group| group.id).unwrap_or_else(|| {
            (1u64..)
                .map(|i| format!("group-{i}"))
                .find(|id| !self.groups.contains_key(id))
                .expect("finite group IDs")
        });
        for member in &members {
            self.placements.remove(&member.key());
        }
        let group_target = LayoutTarget::new(TargetKind::Group, &id);
        self.placements.insert(group_target.key(), placement);
        self.groups.insert(id.clone(), FrameGroup { id, members });
        Some(group_target)
    }

    /// Forget a closed window/deleted item without leaving an invalid one-member group.
    pub fn remove_target(&mut self, target: &LayoutTarget) {
        self.ungroup(target);
        self.placements.remove(&target.key());
    }
}
