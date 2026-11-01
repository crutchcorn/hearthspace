//! wl_subsurface position and stacking belong to the parent's commit, while the
//! child's buffer and extension state belong to its own wl_surface.commit.
use wayland_server::{DisplayHandle, Resource, Weak, protocol::wl_subsurface::WlSubsurface};

use crate::utils::{Logical, Point};

use super::{Location, PrivateSurfaceData, WlSurface};
use crate::wayland::compositor::{SubsurfaceCachedState, cache::Cacheable};

#[derive(Default)]
struct ParentState {
    positions: Vec<Position>,
    stacking: Option<Vec<Weak<WlSurface>>>,
}

struct Position {
    subsurface: Weak<WlSubsurface>,
    surface: Weak<WlSurface>,
    location: Point<i32, Logical>,
}

impl Cacheable for ParentState {
    fn commit(&mut self, _dh: &DisplayHandle) -> Self {
        Self {
            positions: std::mem::take(&mut self.positions),
            // Subsequent requests resolve against the latest requested order,
            // including when its earlier parent commit is still blocked.
            stacking: self.stacking.clone(),
        }
    }

    fn merge_into(self, into: &mut Self, _dh: &DisplayHandle) {
        // Merging may coalesce cached commits before transaction application.
        // Do not change the live tree here.
        into.positions.extend(self.positions);
        if self.stacking.is_some() {
            into.stacking = self.stacking;
        }
    }
}

impl PrivateSurfaceData {
    pub fn set_position(surface: &WlSurface, subsurface: &WlSubsurface, location: Point<i32, Logical>) {
        let Some(parent) = Self::get_parent(surface) else {
            return;
        };
        Self::with_states(&parent, |states| {
            let mut cache = states.cached_state.get::<ParentState>();
            let positions = &mut cache.pending().positions;
            // Repeated position requests before one parent commit need only the
            // final coordinates; ordering relative to stacking is independent.
            if let Some(position) = positions
                .iter_mut()
                .find(|position| position.subsurface == *subsurface)
            {
                position.location = location;
            } else {
                positions.push(Position {
                    subsurface: subsurface.downgrade(),
                    surface: surface.downgrade(),
                    location,
                });
            }
        });
    }

    /// Validate immediately, but apply stacking only with the parent's state.
    pub fn reorder(surface: &WlSurface, to: Location, relative_to: &WlSurface) -> Result<(), ()> {
        let parent = Self::get_parent(surface).ok_or(())?;
        let data = Self::lock_user_data(&parent);
        if surface == relative_to || !data.children.contains(relative_to) {
            return Err(());
        }
        let mut cache = data.public_data.cached_state.get::<ParentState>();
        let stacking = cache
            .pending()
            .stacking
            .get_or_insert_with(|| data.children.iter().map(Resource::downgrade).collect());
        let index = stacking.iter().position(|child| child == surface).ok_or(())?;
        let mut other = stacking.iter().position(|child| child == relative_to).ok_or(())?;
        let surface = stacking.remove(index);
        if index < other {
            other -= 1;
        }
        let destination = match to {
            Location::Before => other,
            Location::After => other + 1,
        };
        stacking.insert(destination, surface);
        Ok(())
    }

    pub(super) fn add_child(&mut self, child: WlSurface) {
        if self.public_data.cached_state.has::<ParentState>() {
            let mut cache = self.public_data.cached_state.get::<ParentState>();
            if let Some(stacking) = &mut cache.pending().stacking {
                stacking.push(child.downgrade());
            }
        }
        self.children.push(child);
    }

    pub(super) fn remove_child(&mut self, child: &WlSurface) {
        self.children.retain(|member| member != child);
        if self.public_data.cached_state.has::<ParentState>() {
            // Destroying a role removes only that member, not surviving moves
            // which were resolved relative to it. Remove it from every cached
            // order so recreating the role cannot revive an old placement.
            self.public_data
                .cached_state
                .get::<ParentState>()
                .override_all(|state| {
                    if let Some(stacking) = &mut state.stacking {
                        stacking.retain(|member| member != child);
                    }
                });
        }
    }

    pub(crate) fn apply_parent_state(parent: &WlSurface) {
        let applied = Self::with_states(parent, |states| {
            if !states.cached_state.has::<ParentState>() {
                return ParentState::default();
            }
            std::mem::take(states.cached_state.get::<ParentState>().current())
        });
        for position in applied.positions {
            // A destroyed/recreated subsurface role must not inherit a queued
            // operation, even when the underlying wl_surface is still alive.
            if !position.subsurface.is_alive() {
                continue;
            }
            let Ok(surface) = position.surface.upgrade() else {
                continue;
            };
            if Self::get_parent(&surface).as_ref() != Some(parent) {
                continue;
            }
            Self::with_states(&surface, |states| {
                states
                    .cached_state
                    .get::<SubsurfaceCachedState>()
                    .override_all(|state| state.location = position.location);
            });
        }
        if let Some(stacking) = applied.stacking {
            let mut data = Self::lock_user_data(parent);
            let mut ordered = Vec::with_capacity(data.children.len());
            for member in stacking {
                if let Some(index) = data.children.iter().position(|child| member == *child) {
                    ordered.push(data.children.remove(index));
                }
            }
            // Roles created after this commit are absent from its snapshot and
            // retain their initial placement above the older surviving members.
            ordered.append(&mut data.children);
            data.children = ordered;
        }
    }
}
