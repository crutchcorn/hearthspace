//! Shared interactive resize policy for server chrome and client requests.
#[cfg(feature = "xwayland")]
use smithay::utils::Rectangle;
use smithay::utils::{Logical, Point, Serial, Size};
use tracing::{debug, trace};
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::protocol::wl_surface::WlSurface;

use super::{ResizeEdges, resize_grid::target_size, resize_limits::ResizeLimits};
use crate::compositor::{App, ManagedSurface, ResizeState};

pub(in crate::compositor) struct PendingResize {
    anchor: ResizeState,
    final_serial: Option<Serial>,
    #[cfg(feature = "xwayland")]
    final_size: Size<i32, Logical>,
}

impl App {
    /// Begin an interactive resize of `window_index` along `edges`, capturing the
    /// window's starting geometry so motion deltas can be resolved against it.
    pub(in crate::compositor) fn start_resize(&mut self, window_index: usize, edges: ResizeEdges) {
        if self.window_uses_output_coordinates(window_index)
            || self.windows[window_index].presentation.maximized
        {
            return;
        }
        self.cancel_group_window_motion(self.windows[window_index].id);
        self.grab_canvas_peek_target(window_index);
        self.prepare_window_pointer_operation();
        let pointer_start = self.window_pointer_start(window_index);
        self.begin_window_resize_at(window_index, edges, pointer_start);
        self.start_window_pointer_grab(window_index);
        if pointer_start != self.raw_pointer_location {
            self.update_resize(self.raw_pointer_location);
        }
        self.request_redraw();
    }

    pub(in crate::compositor) fn begin_window_resize_at(
        &mut self,
        window_index: usize,
        edges: ResizeEdges,
        pointer_start: Point<f64, Logical>,
    ) {
        let initial_bounds = self.window_canvas_rect(window_index);
        let initial_content_size = (
            initial_bounds.size.w,
            initial_bounds.size.h
                - if self.has_compositor_chrome(window_index) {
                    crate::config::TITLE_BAR_HEIGHT
                } else {
                    0
                },
        )
            .into();
        self.windows[window_index].pending_resize = None;
        let window = &self.windows[window_index];
        debug!(
            window_id = window.id,
            ?edges,
            ?initial_content_size,
            "starting interactive resize"
        );
        self.resize = Some(ResizeState {
            window_id: window.id,
            edges,
            pointer_start,
            initial_content_size,
            initial_bounds,
            moved: false,
        });
        self.request_redraw();
    }

    /// Resolve a pointer position during an active resize into a new client
    /// content size and forward it to the window's client.
    pub(in crate::compositor) fn update_resize(&mut self, location: Point<f64, Logical>) {
        let Some(resize) = self.resize.as_mut() else {
            return;
        };
        let edges = resize.edges;
        let initial = resize.initial_content_size;
        let window_id = resize.window_id;
        let delta = location - resize.pointer_start;
        resize.moved |= delta.x.hypot(delta.y) > 0.5;
        if !resize.moved {
            return;
        }
        let Some(window) = self.windows.iter().find(|window| window.id == window_id) else {
            return;
        };
        let limits = ResizeLimits::for_surface(&window.surface);
        let padding = frame_padding(&window.surface);
        let visible_limits = |size: Size<i32, Logical>| {
            (
                size.w.saturating_sub(padding.w).max(1),
                size.h.saturating_sub(padding.h).max(1),
            )
                .into()
        };
        let size = target_size(
            resize.initial_bounds,
            initial,
            edges,
            (delta.x / self.viewport_scale, delta.y / self.viewport_scale).into(),
            visible_limits(limits.min),
            visible_limits(limits.max),
            self.keyboard.modifier_state().alt,
        );
        let full_size = (
            size.w.saturating_add(padding.w),
            size.h.saturating_add(padding.h),
        )
            .into();
        let constrained: Size<i32, Logical> = visible_limits(limits.constrain(full_size));
        // A one-axis resize must not change an untouched dimension.
        let size = (
            if edges.left || edges.right {
                constrained.w
            } else {
                initial.w
            },
            if edges.top || edges.bottom {
                constrained.h
            } else {
                initial.h
            },
        )
            .into();
        trace!(window_id, ?edges, ?size, "configuring interactive resize");
        self.configure_resize(window_id, size);
    }

    /// Send the in-progress resize size to the resized window's client. The
    /// client applies it on its next commit, where the window is re-anchored.
    pub(in crate::compositor) fn configure_resize(&self, window_id: u64, size: Size<i32, Logical>) {
        let Some(window) = self.windows.iter().find(|window| window.id == window_id) else {
            return;
        };
        match &window.surface {
            ManagedSurface::Xdg(surface) => {
                surface.with_pending_state(|state| {
                    state.states.set(xdg_toplevel::State::Resizing);
                    state.bounds = None;
                    state.size = Some(size);
                });
                // Motion can round or clamp to an unchanged logical size.
                // Smithay compares with the latest pending/acknowledged state,
                // so identical samples need not enqueue another client frame.
                crate::compositor::gtk_shell::send_pending_configure(surface);
            }
            #[cfg(feature = "xwayland")]
            ManagedSurface::X11(surface) => {
                let padding = frame_padding(&window.surface);
                let full_size = (
                    size.w.saturating_add(padding.w),
                    size.h.saturating_add(padding.h),
                )
                    .into();
                let rect = self.x11_root_geometry(
                    surface,
                    Rectangle::new(
                        Point::from((window.position.x, window.position.y)),
                        full_size,
                    ),
                );
                if surface.last_configure() == rect {
                    return;
                }
                if let Err(error) = surface.configure(Some(rect)) {
                    tracing::warn!(%error, "failed to configure X11 resize");
                }
            }
        }
    }

    /// End the active interactive resize, clearing the `Resizing` state on the
    /// client so it can return to its normal rendering.
    pub(in crate::compositor) fn finish_resize(&mut self) {
        let Some(resize) = self.resize.take() else {
            return;
        };
        debug!(window_id = resize.window_id, "finishing interactive resize");
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|window| window.id == resize.window_id)
        {
            let final_serial = window.surface.xdg_toplevel().map(|surface| {
                surface.with_pending_state(|state| {
                    state.states.unset(xdg_toplevel::State::Resizing);
                });
                crate::compositor::gtk_shell::send_configure(surface)
            });
            if resize.moved {
                #[cfg(feature = "xwayland")]
                let final_size = match &window.surface {
                    ManagedSurface::Xdg(_) => Size::default(),
                    #[cfg(feature = "xwayland")]
                    ManagedSurface::X11(surface) => surface.last_configure().size,
                };
                window.pending_resize = Some(PendingResize {
                    anchor: resize,
                    final_serial,
                    #[cfg(feature = "xwayland")]
                    final_size,
                });
            }
        }
    }

    /// Re-anchor a window being resized from its left/top edge so the opposite
    /// edge stays put as the client's content size changes. Called on commit,
    /// after the new content bounding box has been cached.
    pub(in crate::compositor) fn reanchor_resize(&mut self, surface: &WlSurface) {
        let Some(window_index) = self.window_index_for_surface(surface) else {
            return;
        };
        let window = &self.windows[window_index];
        let active = self
            .resize
            .as_ref()
            .filter(|resize| resize.window_id == window.id);
        let Some(resize) = active.or_else(|| {
            window
                .pending_resize
                .as_ref()
                .map(|pending| &pending.anchor)
        }) else {
            return;
        };
        let bounds = self.window_canvas_rect(window_index);
        let inset = bounds.loc - Point::from((window.position.x, window.position.y));
        let mut position = crate::geometry::CanvasPoint {
            x: resize.initial_bounds.loc.x.saturating_sub(inset.x),
            y: resize.initial_bounds.loc.y.saturating_sub(inset.y),
        };
        if resize.edges.left {
            position.x = (i64::from(resize.initial_bounds.loc.x)
                + i64::from(resize.initial_bounds.size.w)
                - i64::from(bounds.size.w)
                - i64::from(inset.x))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        }
        if resize.edges.top {
            position.y = (i64::from(resize.initial_bounds.loc.y)
                + i64::from(resize.initial_bounds.size.h)
                - i64::from(bounds.size.h)
                - i64::from(inset.y))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        }
        let finished = active.is_none()
            && window.pending_resize.as_ref().is_some_and(|pending| {
                match (&window.surface, pending.final_serial) {
                    (ManagedSurface::Xdg(surface), Some(serial)) => {
                        surface.with_cached_state(|state| {
                            state
                                .last_acked
                                .as_ref()
                                .is_some_and(|configure| configure.serial >= serial)
                        })
                    }
                    // X11 has no xdg configure serial; wait for the final geometry.
                    #[cfg(feature = "xwayland")]
                    (ManagedSurface::X11(surface), _) => surface.bbox().size == pending.final_size,
                    _ => false,
                }
            });
        if self.windows[window_index].position != position {
            self.windows[window_index].position = position;
            #[cfg(feature = "xwayland")]
            self.sync_x11_window_position(self.windows[window_index].id);
        }
        if finished {
            self.windows[window_index].pending_resize = None;
        }
    }
}

fn frame_padding(surface: &ManagedSurface) -> Size<i32, Logical> {
    match surface {
        ManagedSurface::Xdg(_) => Size::default(),
        #[cfg(feature = "xwayland")]
        ManagedSurface::X11(surface) => {
            let full = surface.bbox().size;
            let visible = surface.geometry().size;
            (
                full.w.saturating_sub(visible.w).max(0),
                full.h.saturating_sub(visible.h).max(0),
            )
                .into()
        }
    }
}
