//! GTK v2+ consumes fresh edge constraints alongside every xdg configure.
use std::sync::Mutex;

use smithay::{
    utils::Serial,
    wayland::{
        compositor::with_states,
        shell::xdg::{SurfaceCachedState, ToplevelSurface},
    },
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::{Resource, Weak, protocol::wl_surface::WlSurface};

use super::protocol::gtk_surface1::{EdgeConstraint, GtkSurface1};
use crate::compositor::App;

#[derive(Default)]
struct GtkSurfaces(Mutex<SurfaceState>);

#[derive(Default)]
struct SurfaceState {
    resources: Vec<Weak<GtkSurface1>>,
    edges: Option<ResizeEdges>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ResizeEdges {
    horizontal: bool,
    vertical: bool,
}

impl ResizeEdges {
    fn for_toplevel(surface: &ToplevelSurface) -> Self {
        let pinned = surface.with_pending_state(|state| {
            state.states.contains(xdg_toplevel::State::Maximized)
                || state.states.contains(xdg_toplevel::State::Fullscreen)
        });
        let hints = with_states(surface.wl_surface(), |states| {
            *states.cached_state.get::<SurfaceCachedState>().current()
        });
        Self {
            horizontal: !pinned && (hints.max_size.w == 0 || hints.min_size.w != hints.max_size.w),
            vertical: !pinned && (hints.max_size.h == 0 || hints.min_size.h != hints.max_size.h),
        }
    }

    fn bytes(self) -> Vec<u8> {
        [
            (self.vertical, EdgeConstraint::ResizableTop),
            (self.horizontal, EdgeConstraint::ResizableRight),
            (self.vertical, EdgeConstraint::ResizableBottom),
            (self.horizontal, EdgeConstraint::ResizableLeft),
        ]
        .into_iter()
        .filter(|(enabled, _)| *enabled)
        .flat_map(|(_, edge)| (edge as u32).to_ne_bytes())
        .collect()
    }
}

pub(super) fn track(surface: &WlSurface, gtk: &GtkSurface1) {
    with_states(surface, |states| {
        states.data_map.insert_if_missing(GtkSurfaces::default);
        let mut state = states
            .data_map
            .get::<GtkSurfaces>()
            .unwrap()
            .0
            .lock()
            .unwrap();
        state
            .resources
            .retain(|resource| resource.upgrade().is_ok());
        state.resources.push(gtk.downgrade());
    });
}

pub(in crate::compositor) fn send_configure(surface: &ToplevelSurface) -> Serial {
    let edges = ResizeEdges::for_toplevel(surface);
    let resources = with_states(surface.wl_surface(), |states| {
        states
            .data_map
            .get::<GtkSurfaces>()
            .map(|data| {
                let mut state = data.0.lock().unwrap();
                state.edges = Some(edges);
                state
                    .resources
                    .retain(|resource| resource.upgrade().is_ok());
                state
                    .resources
                    .iter()
                    .filter_map(|resource| resource.upgrade().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    if !resources.is_empty() {
        let edges = edges.bytes();
        for gtk in resources {
            // Canvas maximization is not edge tiling. Xdg-shell carries its
            // maximize/fullscreen flags; GTK's extra tiled state stays empty.
            gtk.configure(Vec::new());
            if gtk.version() >= 2 {
                gtk.configure_edges(edges.clone());
            }
        }
    }
    surface.send_configure()
}

impl App {
    pub(in crate::compositor) fn refresh_gtk_resize_edges(&self, root: &WlSurface) {
        let Some(toplevel) = self
            .windows
            .iter()
            .filter_map(|window| window.surface.xdg_toplevel())
            .find(|toplevel| toplevel.wl_surface() == root)
            .filter(|toplevel| toplevel.is_initial_configure_sent())
        else {
            return;
        };
        let previous = with_states(root, |states| {
            let mut state = states.data_map.get::<GtkSurfaces>()?.0.lock().unwrap();
            state
                .resources
                .retain(|resource| resource.upgrade().is_ok());
            state
                .resources
                .iter()
                .filter_map(|resource| resource.upgrade().ok())
                .any(|resource| resource.version() >= 2)
                .then_some(state.edges)
                .flatten()
        });
        if previous.is_some_and(|previous| previous != ResizeEdges::for_toplevel(toplevel)) {
            // Only an effective edge change needs a new sequence. The commit
            // acknowledging it sees the same mask and cannot trigger a loop.
            send_configure(toplevel);
        }
    }
}

pub(in crate::compositor) fn send_pending_configure(surface: &ToplevelSurface) -> Option<Serial> {
    surface
        .has_pending_changes()
        .then(|| send_configure(surface))
}
