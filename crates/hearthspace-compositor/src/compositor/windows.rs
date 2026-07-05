use smithay::{
    desktop::utils::bbox_from_surface_tree,
    utils::{Logical, Physical, Point, Rectangle, SERIAL_COUNTER, Size},
    wayland::{
        compositor::{TraversalAction, with_states, with_surface_tree_downward},
        shell::xdg::{ToplevelSurface, XdgToplevelSurfaceData},
    },
};
use tracing::{debug, trace};
use wayland_protocols::xdg::{
    decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode,
    shell::server::xdg_toplevel,
};
use wayland_server::protocol::wl_surface::{self, WlSurface};

use crate::{accessibility::ManagedWindowAccessibilityInfo, config::*, geometry::CanvasPoint};
use hearthspace_common::{launcher_output_name_from_app_id, shell_bar_output_name_from_app_id};

mod geometry;
mod hit_testing;
mod ordering;

pub(super) use geometry::{ResizeEdges, resize_cursor_icon};
use ordering::normal_insert_index_for_kinds;

use geometry::{
    close_button_canvas_rect_for, content_canvas_origin_for, resize_anchored_position,
    resize_target_content_size, title_bar_canvas_rect_for, window_canvas_rect_for,
};

use super::{
    App, ManagedWindow, ManagedWindowKind, ResizeState, WindowDecoration,
    idle::ActivityReason,
    rendering::{toplevel_geometry_loc, toplevel_geometry_size},
};

fn configure_server_side_decoration(toplevel: &ToplevelSurface) {
    if window_kind_for_toplevel(toplevel).is_shell_chrome() {
        return;
    }

    toplevel.with_pending_state(|state| {
        state.decoration_mode = Some(DecorationMode::ServerSide);
    });
    toplevel.send_configure();
}

fn configure_client_side_decoration(toplevel: &ToplevelSurface) {
    toplevel.with_pending_state(|state| {
        state.decoration_mode = Some(DecorationMode::ClientSide);
    });
    toplevel.send_configure();
}

pub(super) fn window_kind_for_toplevel(surface: &ToplevelSurface) -> ManagedWindowKind {
    let app_id = toplevel_app_id(surface);
    if app_id
        .as_deref()
        .and_then(shell_bar_output_name_from_app_id)
        .is_some()
    {
        return ManagedWindowKind::ShellBar;
    }
    if app_id
        .as_deref()
        .and_then(launcher_output_name_from_app_id)
        .is_some()
    {
        return ManagedWindowKind::Launcher;
    }
    if app_id.as_deref() == Some(SHELL_OUTPUT_WATCHER_APP_ID) {
        return ManagedWindowKind::ShellWatcher;
    }
    ManagedWindowKind::Normal
}

pub(super) fn shell_output_name_for_toplevel(surface: &ToplevelSurface) -> Option<String> {
    let app_id = toplevel_app_id(surface)?;
    shell_bar_output_name_from_app_id(&app_id)
        .or_else(|| launcher_output_name_from_app_id(&app_id))
        .flatten()
}

fn toplevel_app_id(surface: &ToplevelSurface) -> Option<String> {
    with_states(surface.wl_surface(), |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|data| data.lock().ok()?.app_id.clone())
    })
}

pub(super) fn toplevel_title(surface: &ToplevelSurface) -> Option<String> {
    with_states(surface.wl_surface(), |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|data| data.lock().ok()?.title.clone())
    })
}

pub(super) fn position_for_new_window(
    kind: ManagedWindowKind,
    fallback: CanvasPoint,
) -> CanvasPoint {
    match kind {
        ManagedWindowKind::Normal => fallback,
        ManagedWindowKind::ShellBar => CanvasPoint { x: 0, y: 0 },
        ManagedWindowKind::ShellWatcher => CanvasPoint { x: 0, y: 0 },
        ManagedWindowKind::Launcher => CanvasPoint {
            x: 0,
            y: CONTROL_BAR_HEIGHT,
        },
    }
}

pub(super) fn decoration_for_new_window(kind: ManagedWindowKind) -> WindowDecoration {
    match kind {
        ManagedWindowKind::Normal
        | ManagedWindowKind::ShellBar
        | ManagedWindowKind::Launcher
        | ManagedWindowKind::ShellWatcher => WindowDecoration::ClientSide,
    }
}

impl App {
    pub(super) fn set_keyboard_focus_to_window(&mut self, window_index: usize, surface: WlSurface) {
        debug!(window_id = self.windows[window_index].id, kind = ?self.windows[window_index].kind, "setting keyboard focus to window");
        self.focused_normal_window_id = (self.windows[window_index].kind
            == ManagedWindowKind::Normal)
            .then_some(self.windows[window_index].id);
        let keyboard = self.keyboard.clone();
        keyboard.set_focus(self, Some(surface), SERIAL_COUNTER.next_serial());
    }

    pub(super) fn clear_keyboard_focus(&mut self) {
        debug!("clearing keyboard focus");
        self.focused_normal_window_id = None;
        let keyboard = self.keyboard.clone();
        keyboard.set_focus(
            self,
            Option::<WlSurface>::None,
            SERIAL_COUNTER.next_serial(),
        );
    }

    pub(super) fn record_client_activity_for_window_index(
        &self,
        window_index: usize,
        reason: ActivityReason,
    ) {
        let Some(window) = self.windows.get(window_index) else {
            return;
        };
        if window.kind == ManagedWindowKind::Normal {
            self.idle_daemon.record_activity(window.id, reason);
        }
    }

    pub(super) fn record_focused_client_activity(&self, reason: ActivityReason) {
        if let Some(window_id) = self.focused_normal_window_id {
            self.idle_daemon.record_activity(window_id, reason);
        }
    }

    pub(super) fn window_index_for_surface(&self, surface: &WlSurface) -> Option<usize> {
        self.windows
            .iter()
            .position(|window| surface_tree_contains(window.surface.wl_surface(), surface))
    }

    pub(super) fn window_mut_by_id(&mut self, window_id: u64) -> Option<&mut ManagedWindow> {
        self.windows
            .iter_mut()
            .find(|window| window.id == window_id)
    }

    pub(super) fn managed_normal_window_id_for_surface(&self, surface: &WlSurface) -> Option<u64> {
        self.windows.iter().find_map(|window| {
            (window.kind == ManagedWindowKind::Normal
                && surface_tree_contains(window.surface.wl_surface(), surface))
            .then_some(window.id)
        })
    }

    /// Recompute and cache the surface-tree bounding box for the window owning
    /// `surface`. Called once per commit so the per-frame rendering and
    /// hit-testing paths can read `content_bbox_size` instead of re-walking the
    /// tree multiple times.
    pub(super) fn refresh_window_content_bbox(&mut self, surface: &WlSurface) {
        let Some(window_index) = self.window_index_for_surface(surface) else {
            return;
        };
        let bbox = bbox_from_surface_tree(
            self.windows[window_index].surface.wl_surface(),
            Point::<i32, Logical>::from((0, 0)),
        );
        self.windows[window_index].content_bbox_size = bbox.size;
    }

    pub(super) fn handle_idle_transitions(&self) {
        for transition in self.idle_daemon.drain_transitions() {
            super::log_idle_transition(transition);
        }
    }

    pub(super) fn accessibility_window_snapshot(&self) -> Vec<ManagedWindowAccessibilityInfo> {
        self.windows
            .iter()
            .filter(|window| window.kind == ManagedWindowKind::Normal)
            .map(|window| ManagedWindowAccessibilityInfo {
                id: window.id,
                app_id: toplevel_app_id(&window.surface),
                title: toplevel_title(&window.surface),
            })
            .collect()
    }

    pub(super) fn raise_window(&mut self, window_index: usize) -> usize {
        if self.windows[window_index].kind != ManagedWindowKind::Normal {
            return window_index;
        }

        let window = self.windows.remove(window_index);
        let window_id = window.id;
        let insert_index = self.normal_insert_index();
        self.windows.insert(insert_index, window);
        debug!(
            window_id,
            from_index = window_index,
            to_index = insert_index,
            "raised window"
        );
        self.request_redraw();
        insert_index
    }

    pub(super) fn normal_insert_index(&self) -> usize {
        normal_insert_index_for_kinds(self.windows.iter().map(|window| window.kind))
    }

    pub(super) fn configure_toplevel(
        &self,
        surface: &ToplevelSurface,
        kind: ManagedWindowKind,
        shell_output_name: Option<&str>,
    ) {
        trace!(?kind, "configuring xdg toplevel");
        surface.with_pending_state(|state| {
            state
                .capabilities
                .replace(std::iter::empty::<xdg_toplevel::WmCapabilities>());
            match kind {
                ManagedWindowKind::ShellBar => {
                    let width = self
                        .output_client_logical_size_for_name(shell_output_name)
                        .map(|size| size.w)
                        .unwrap_or_else(|| self.minimum_output_client_logical_width());
                    trace!(
                        width,
                        ?shell_output_name,
                        "configuring shell bar to output width"
                    );
                    state.size = Some((width, CONTROL_BAR_HEIGHT).into());
                    state.bounds = Some((width, CONTROL_BAR_HEIGHT).into());
                    state.decoration_mode = Some(DecorationMode::ClientSide);
                }
                ManagedWindowKind::Launcher => {
                    // The launcher sizes itself to its result list, so the
                    // compositor leaves the size unset (client-driven) and only
                    // enforces client-side decorations.
                    state.decoration_mode = Some(DecorationMode::ClientSide);
                }
                ManagedWindowKind::ShellWatcher => {
                    state.size = Some((1, 1).into());
                    state.bounds = Some((1, 1).into());
                    state.decoration_mode = Some(DecorationMode::ClientSide);
                }
                ManagedWindowKind::Normal => {
                    state.states.set(xdg_toplevel::State::Activated);
                }
            }
        });
        surface.send_configure();
    }

    #[cfg_attr(not(feature = "winit"), allow(dead_code))]
    pub(super) fn configure_shell_bars(&self) {
        for window in &self.windows {
            if window.kind.is_shell_chrome() {
                self.enter_shell_output(
                    window.surface.wl_surface(),
                    window.shell_output_name.as_deref(),
                );
            }
            if window.kind == ManagedWindowKind::ShellBar {
                self.configure_toplevel(
                    &window.surface,
                    window.kind,
                    window.shell_output_name.as_deref(),
                );
            }
        }
    }

    pub(super) fn request_redraw(&mut self) {
        self.needs_redraw = true;
    }

    pub(super) fn set_window_decoration(
        &mut self,
        toplevel: &ToplevelSurface,
        decoration: WindowDecoration,
    ) {
        if window_kind_for_toplevel(toplevel).is_shell_chrome() {
            configure_client_side_decoration(toplevel);
            return;
        }

        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|window| window.surface == *toplevel)
        {
            window.decoration = decoration;
        }

        match decoration {
            WindowDecoration::ServerSide => configure_server_side_decoration(toplevel),
            WindowDecoration::ClientSide => configure_client_side_decoration(toplevel),
        }
    }

    pub(super) fn title_bar_rect(&self, window_index: usize) -> Rectangle<i32, Logical> {
        let canvas_rect = self.title_bar_canvas_rect(window_index);
        let origin = self
            .canvas_to_screen(canvas_rect.loc.to_f64())
            .to_i32_round();
        Rectangle::new(
            origin,
            (
                (f64::from(canvas_rect.size.w) * self.viewport_scale).round() as i32,
                (f64::from(canvas_rect.size.h) * self.viewport_scale).round() as i32,
            )
                .into(),
        )
    }

    pub(super) fn title_bar_canvas_rect(&self, window_index: usize) -> Rectangle<i32, Logical> {
        let window = &self.windows[window_index];
        title_bar_canvas_rect_for(window.position, window.content_bbox_size.w)
    }

    fn close_button_canvas_rect(&self, window_index: usize) -> Rectangle<i32, Logical> {
        close_button_canvas_rect_for(self.title_bar_canvas_rect(window_index))
    }

    fn content_canvas_origin(&self, window_index: usize) -> Point<i32, Logical> {
        content_canvas_origin_for(
            self.windows[window_index].position,
            self.has_compositor_chrome(window_index),
        )
    }

    pub(super) fn has_compositor_chrome(&self, window_index: usize) -> bool {
        let window = &self.windows[window_index];
        window.kind == ManagedWindowKind::Normal
            && window.decoration == WindowDecoration::ServerSide
    }

    /// The window's interactive size: the xdg window geometry size when the
    /// client has set one (client-side-decorated apps inset their visible window
    /// from the surface bounds with shadow margins), falling back to the
    /// surface-tree bounding box otherwise.
    fn window_geometry_size(&self, window_index: usize) -> Size<i32, Logical> {
        let window = &self.windows[window_index];
        toplevel_geometry_size(window.surface.wl_surface())
            .filter(|size| size.w > 0 && size.h > 0)
            .unwrap_or(window.content_bbox_size)
    }

    /// Full canvas-space bounds of a window's interactive area, including its
    /// server-side title bar when present. For client-side-decorated windows the
    /// rect tracks the *visible* window geometry (excluding shadow margins) so
    /// resize handles land on the visible edge rather than out in the shadow.
    fn window_canvas_rect(&self, window_index: usize) -> Rectangle<i32, Logical> {
        let window = &self.windows[window_index];
        if self.has_compositor_chrome(window_index) {
            return window_canvas_rect_for(window.position, window.content_bbox_size, true);
        }
        let surface_origin = self.content_canvas_origin(window_index);
        let geometry_loc = toplevel_geometry_loc(window.surface.wl_surface());
        Rectangle::new(
            surface_origin + geometry_loc,
            self.window_geometry_size(window_index),
        )
    }

    /// Begin an interactive resize of `window_index` along `edges`, capturing the
    /// window's starting geometry so motion deltas can be resolved against it.
    pub(super) fn start_resize(&mut self, window_index: usize, edges: ResizeEdges) {
        let initial_content_size = self.window_geometry_size(window_index);
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
            pointer_start: self.raw_pointer_location,
            initial_position: window.position,
            initial_content_size,
        });
        self.request_redraw();
    }

    /// Resolve a pointer position during an active resize into a new client
    /// content size and forward it to the window's client.
    pub(super) fn update_resize(&mut self, location: Point<f64, Logical>) {
        let Some(resize) = self.resize.as_ref() else {
            return;
        };
        let edges = resize.edges;
        let initial = resize.initial_content_size;
        let window_id = resize.window_id;
        let delta = location - resize.pointer_start;
        let canvas_delta = Point::<i32, Logical>::from((
            (delta.x / self.viewport_scale).round() as i32,
            (delta.y / self.viewport_scale).round() as i32,
        ));
        let size = resize_target_content_size(edges, initial, canvas_delta);
        trace!(window_id, ?edges, ?size, "configuring interactive resize");
        self.configure_resize(window_id, size);
    }

    /// Send the in-progress resize size to the resized window's client. The
    /// client applies it on its next commit, where the window is re-anchored.
    pub(super) fn configure_resize(&self, window_id: u64, size: Size<i32, Logical>) {
        let Some(window) = self.windows.iter().find(|window| window.id == window_id) else {
            return;
        };
        window.surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Resizing);
            state.size = Some(size);
        });
        window.surface.send_configure();
    }

    /// End the active interactive resize, clearing the `Resizing` state on the
    /// client so it can return to its normal rendering.
    pub(super) fn finish_resize(&mut self) {
        let Some(resize) = self.resize.take() else {
            return;
        };
        debug!(window_id = resize.window_id, "finishing interactive resize");
        if let Some(window) = self
            .windows
            .iter()
            .find(|window| window.id == resize.window_id)
        {
            window.surface.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Resizing);
            });
            window.surface.send_configure();
        }
    }

    /// Re-anchor a window being resized from its left/top edge so the opposite
    /// edge stays put as the client's content size changes. Called on commit,
    /// after the new content bounding box has been cached.
    pub(super) fn reanchor_resize(&mut self, surface: &WlSurface) {
        let Some(resize) = self.resize.as_ref() else {
            return;
        };
        if !(resize.edges.left || resize.edges.top) {
            return;
        }
        let Some(window_index) = self.window_index_for_surface(surface) else {
            return;
        };
        if self.windows[window_index].id != resize.window_id {
            return;
        }
        let new_position = resize_anchored_position(
            resize.edges,
            resize.initial_position,
            resize.initial_content_size,
            self.window_geometry_size(window_index),
        );
        self.windows[window_index].position = new_position;
    }
    fn content_screen_origin(&self, window_index: usize) -> Point<i32, Physical> {
        self.canvas_to_screen(self.content_canvas_origin(window_index).to_f64())
            .to_i32_round()
            .to_physical(1)
    }

    fn shell_bar_screen_origin(&self, window_index: usize) -> Option<Point<i32, Physical>> {
        self.shell_bar_logical_origin(window_index)
            .map(|origin| origin.to_physical(1))
    }

    pub(super) fn shell_bar_screen_origins(
        &self,
        window_index: usize,
    ) -> Vec<Point<i32, Physical>> {
        self.shell_bar_screen_origin(window_index)
            .into_iter()
            .collect()
    }

    pub(super) fn launcher_screen_origins(&self, window_index: usize) -> Vec<Point<i32, Physical>> {
        self.launcher_logical_origin(window_index)
            .map(|origin| origin.to_physical(1))
            .into_iter()
            .collect()
    }

    fn shell_bar_logical_origin(&self, window_index: usize) -> Option<Point<i32, Logical>> {
        let output_name = self.windows[window_index].shell_output_name.as_deref();
        self.output_logical_rect_for_name(output_name)
            .map(|rect| rect.loc)
    }

    /// Logical screen-space origin of a launcher palette: pinned below its
    /// output's shell bar.
    fn launcher_logical_origin(&self, window_index: usize) -> Option<Point<i32, Logical>> {
        let scale =
            self.output_scale_for_name(self.windows[window_index].shell_output_name.as_deref());
        self.shell_bar_logical_origin(window_index)
            .map(|origin| launcher_logical_origin_for_shell_bar(origin, scale))
    }

    pub(super) fn surface_screen_origin(&self, window_index: usize) -> Point<i32, Physical> {
        match self.windows[window_index].kind {
            ManagedWindowKind::Normal => self.content_screen_origin(window_index),
            ManagedWindowKind::ShellBar => self
                .shell_bar_screen_origin(window_index)
                .unwrap_or_else(|| Point::from((0, 0))),
            ManagedWindowKind::ShellWatcher => Point::from((0, 0)),
            ManagedWindowKind::Launcher => self
                .launcher_logical_origin(window_index)
                .unwrap_or_else(|| Point::from((0, CONTROL_BAR_HEIGHT)))
                .to_physical(1),
        }
    }
}

fn launcher_logical_origin_for_shell_bar(
    origin: Point<i32, Logical>,
    output_scale: i32,
) -> Point<i32, Logical> {
    Point::from((
        origin.x,
        origin.y + CONTROL_BAR_HEIGHT * output_scale.max(1),
    ))
}

fn surface_tree_contains(root: &wl_surface::WlSurface, target: &wl_surface::WlSurface) -> bool {
    let mut contains = false;
    with_surface_tree_downward(
        root,
        (),
        |surface, _, &()| {
            if surface == target {
                contains = true;
                TraversalAction::Break
            } else {
                TraversalAction::DoChildren(())
            }
        },
        |_, _, &()| {},
        |_, _, &()| true,
    );
    contains
}

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Point, Rectangle};

    use super::launcher_logical_origin_for_shell_bar;

    #[test]
    fn launcher_origin_sits_below_its_output_shell_bar() {
        let output = Rectangle::<i32, Logical>::new((1280, 120).into(), (1024, 768).into());

        assert_eq!(
            launcher_logical_origin_for_shell_bar(output.loc, 1),
            Point::from((1280, 168))
        );
    }

    #[test]
    fn launcher_origin_sits_below_scaled_output_shell_bar() {
        assert_eq!(
            launcher_logical_origin_for_shell_bar(Point::from((0, 10)), 3),
            Point::from((0, 154))
        );
    }
}
